//! The space model — a directory of files becomes a live, safely-served space
//! (Wave 2a / Phase 2). Production, security-sensitive code.
//!
//! A **space** is a directory of artifacts (`.html`) plus first-class `assets/`.
//! `scan_dir` reads the whole tree into an **immutable in-memory snapshot**;
//! `ArtifactHost` swaps snapshots atomically (a half-written file is never served
//! — reads see either the fully-old or the fully-new snapshot, never a partial
//! one). All of the untrusted-input handling lives here:
//!
//! * **slug grammar** + **reserved-name / collision** hard-error rejection;
//! * **symlink + path-traversal rejection** — every entry is `lstat`-checked and
//!   its canonical path must stay under the space root; assets are matched against
//!   the pre-scanned map (an exact allowlist), so a request path can only ever
//!   retrieve a real, already-vetted file;
//! * **MIME detection** (extension allowlist) with `nosniff`;
//! * **per-file and per-space size limits**;
//! * **title resolution** — parsed (a small tokenizer, not a regex), entity-
//!   decoded, length-bounded; inserted downstream as **text**, never `innerHTML`.
//!
//! See `issues/html-artifact-host-rewrite/{design,plan,wave-plan}.md`.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use super::{valid_name, RESERVED};

/// Per-file byte ceiling. A single artifact/asset larger than this is a hard
/// error — a local dev tool has no business streaming huge blobs, and an
/// unbounded read is a trivial memory-exhaustion foot-gun. Enforced on the
/// **actual bytes read**, not the pre-read `stat` length (which a concurrent
/// write could grow).
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024; // 8 MiB
/// Per-space aggregate ceiling across every artifact + asset.
pub const MAX_SPACE_BYTES: u64 = 64 * 1024 * 1024; // 64 MiB
/// Maximum number of scanned entries (artifacts + assets). Bounds map/CPU blowup
/// from a directory of many tiny files that slips under the byte ceilings.
pub const MAX_ENTRIES: usize = 10_000;
/// Manifest input ceiling before YAML parsing — small, to bound alias-expansion
/// ("billion laughs") blast radius. `glasspad.yaml` is structure-only and tiny.
pub const MAX_MANIFEST_BYTES: u64 = 64 * 1024; // 64 KiB
/// Upper bound on the resolved title length (in chars) inserted into the chrome.
pub const MAX_TITLE_CHARS: usize = 200;
/// The reserved subdirectory that holds a space's static assets. It is also a
/// reserved *slug* name (see `RESERVED`), so no artifact can collide with it.
pub const ASSETS_DIR: &str = "assets";
/// Optional structure-only manifest filename.
pub const MANIFEST_FILE: &str = "glasspad.yaml";

/// One HTML artifact within a space. `html` is the raw file content served
/// verbatim on the content route (fragment wrapping is Wave 3a). `title` is the
/// resolved, entity-decoded, length-bounded display title.
#[derive(Clone, Debug)]
pub struct Artifact {
    pub html: String,
    pub title: String,
}

/// One static asset addressed by its space-relative path (e.g. `assets/logo.svg`).
#[derive(Clone, Debug)]
pub struct Asset {
    pub content_type: &'static str,
    pub bytes: Vec<u8>,
}

/// One fully-scanned space. Immutable once built; shared behind an `Arc`.
#[derive(Clone, Debug, Default)]
pub struct Space {
    /// slug → artifact, ordered lexicographically (the fallback nav order).
    pub artifacts: BTreeMap<String, Artifact>,
    /// space-relative path → asset (e.g. `assets/data.json`).
    pub assets: BTreeMap<String, Asset>,
    /// Ordered slugs for navigation (manifest `nav:` order, else lexicographic).
    pub nav: Vec<String>,
    /// Resolved home slug (`index` > `home` > first in nav order).
    pub home: Option<String>,
    /// Optional space title from the manifest (structure, never content).
    pub title: Option<String>,
}

impl Space {
    pub fn artifact(&self, slug: &str) -> Option<&Artifact> {
        self.artifacts.get(slug)
    }
    pub fn asset(&self, rel_path: &str) -> Option<&Asset> {
        self.assets.get(rel_path)
    }
    pub fn slugs(&self) -> Vec<String> {
        self.nav.clone()
    }
}

/// An immutable set of spaces. Swapped atomically on rescan.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub spaces: BTreeMap<String, Space>,
}

impl Snapshot {
    pub fn empty() -> Self {
        Self::default()
    }
    pub fn space(&self, name: &str) -> Option<&Space> {
        self.spaces.get(name)
    }
}

/// Everything that can go wrong while scanning a directory into a space. Each
/// variant renders an **informative** message (AI-first CLI contract).
#[derive(Debug)]
pub enum ScanError {
    NotADir(PathBuf),
    BadSpaceName(String),
    Io(PathBuf, std::io::Error),
    Symlink(PathBuf),
    Escapes(PathBuf),
    ReservedSlug(String, PathBuf),
    BadSlug(String, PathBuf),
    DuplicateSlug(String, PathBuf),
    FileTooLarge(PathBuf, u64),
    SpaceTooLarge(u64),
    TooManyEntries(usize),
    UnsupportedFileType(PathBuf),
    ManifestTooLarge(PathBuf, u64),
    NotUtf8(PathBuf),
    BadAssetName(PathBuf),
    Manifest(PathBuf, String),
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScanError::NotADir(p) => write!(f, "not a directory: {}", p.display()),
            ScanError::BadSpaceName(n) => write!(
                f,
                "invalid space name {n:?}: must be lowercase [a-z0-9-], start alphanumeric, \
                 ≤64 chars, and not a reserved name ({})",
                RESERVED.join(", ")
            ),
            ScanError::Io(p, e) => write!(f, "cannot read {}: {e}", p.display()),
            ScanError::Symlink(p) => write!(
                f,
                "refusing to serve symlink {} (symlinks are rejected: they can point outside the space)",
                p.display()
            ),
            ScanError::Escapes(p) => write!(
                f,
                "refusing {}: resolves outside the space root (path traversal)",
                p.display()
            ),
            ScanError::ReservedSlug(s, p) => write!(
                f,
                "reserved slug {s:?} ({}): the names {} are reserved and cannot be artifact slugs",
                p.display(),
                RESERVED.join(", ")
            ),
            ScanError::BadSlug(s, p) => write!(
                f,
                "invalid slug {s:?} ({}): a slug (filename stem) must be lowercase [a-z0-9-], \
                 start alphanumeric, and be ≤64 chars",
                p.display()
            ),
            ScanError::DuplicateSlug(s, p) => write!(
                f,
                "duplicate slug {s:?} ({}): two files map to the same slug — rename one \
                 (collisions are never silently resolved)",
                p.display()
            ),
            ScanError::FileTooLarge(p, n) => write!(
                f,
                "{} is {n} bytes, over the {MAX_FILE_BYTES}-byte per-file limit",
                p.display()
            ),
            ScanError::SpaceTooLarge(n) => write!(
                f,
                "space totals {n} bytes, over the {MAX_SPACE_BYTES}-byte per-space limit"
            ),
            ScanError::TooManyEntries(n) => write!(
                f,
                "space has more than {MAX_ENTRIES} files (counted {n}); split it or prune assets"
            ),
            ScanError::UnsupportedFileType(p) => write!(
                f,
                "{} is not a regular file (FIFOs, sockets, and devices are not servable)",
                p.display()
            ),
            ScanError::ManifestTooLarge(p, n) => write!(
                f,
                "{} is {n} bytes, over the {MAX_MANIFEST_BYTES}-byte manifest limit",
                p.display()
            ),
            ScanError::NotUtf8(p) => write!(f, "{} is not valid UTF-8 (artifacts must be UTF-8 HTML)", p.display()),
            ScanError::BadAssetName(p) => write!(
                f,
                "invalid asset path {}: each segment must be [A-Za-z0-9._-], no '.'/'..'/empty segments",
                p.display()
            ),
            ScanError::Manifest(p, e) => write!(f, "cannot parse {}: {e}", p.display()),
        }
    }
}

impl std::error::Error for ScanError {}

/// Derive a space name from a directory path (its final component) and validate
/// it against the space grammar + reserved list.
pub fn space_name_for(dir: &Path) -> Result<String, ScanError> {
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ScanError::BadSpaceName(dir.display().to_string()))?;
    if !super::valid_space(&name) {
        return Err(ScanError::BadSpaceName(name));
    }
    Ok(name)
}

/// Scan a single directory into a `Space`. All-or-nothing: on any rejected entry
/// this returns `Err` and no partial space is produced (the caller keeps serving
/// the previous snapshot). Never follows symlinks; never escapes `root`.
pub fn scan_dir(root: &Path) -> Result<Space, ScanError> {
    let root = root.to_path_buf();
    let meta = std::fs::symlink_metadata(&root).map_err(|e| ScanError::Io(root.clone(), e))?;
    if meta.file_type().is_symlink() {
        return Err(ScanError::Symlink(root));
    }
    if !meta.is_dir() {
        return Err(ScanError::NotADir(root));
    }
    // Canonical root: every accepted file's canonical path must stay under it.
    let canon_root = std::fs::canonicalize(&root).map_err(|e| ScanError::Io(root.clone(), e))?;

    let mut space = Space::default();
    let mut total: u64 = 0;

    // --- top-level entries: *.html artifacts, assets/ dir, manifest ---------
    let mut entries: Vec<_> = std::fs::read_dir(&root)
        .map_err(|e| ScanError::Io(root.clone(), e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ScanError::Io(root.clone(), e))?;
    // Deterministic order so slug-collision / "first wins" decisions are stable.
    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let path = entry.path();
        let ftype = entry.file_type().map_err(|e| ScanError::Io(path.clone(), e))?;
        if ftype.is_symlink() {
            return Err(ScanError::Symlink(path));
        }
        let name = entry.file_name();
        let name = name.to_str().ok_or_else(|| ScanError::BadAssetName(path.clone()))?;

        if ftype.is_dir() {
            if name == ASSETS_DIR {
                scan_assets(&path, &canon_root, &mut space, &mut total)?;
            }
            // Any other subdirectory is ignored (only assets/ is served).
            continue;
        }

        if name == MANIFEST_FILE {
            // Bound the manifest tightly (stat first, then verify actual bytes) so
            // an alias-bomb never reaches the YAML parser.
            if ftype.is_file() && entry_len(&path) > MAX_MANIFEST_BYTES {
                return Err(ScanError::ManifestTooLarge(path.clone(), entry_len(&path)));
            }
            let raw = read_file_capped(&path, &mut total)?;
            if raw.len() as u64 > MAX_MANIFEST_BYTES {
                return Err(ScanError::ManifestTooLarge(path.clone(), raw.len() as u64));
            }
            let text = String::from_utf8(raw).map_err(|_| ScanError::NotUtf8(path.clone()))?;
            apply_manifest(&text, &path, &mut space)?;
            continue;
        }

        // Artifacts are top-level *.html files. Slug = filename stem, literally.
        if let Some(stem) = html_stem(name) {
            if RESERVED.contains(&stem) {
                return Err(ScanError::ReservedSlug(stem.to_string(), path.clone()));
            }
            if !valid_name(stem) {
                return Err(ScanError::BadSlug(stem.to_string(), path.clone()));
            }
            if space.artifacts.contains_key(stem) {
                return Err(ScanError::DuplicateSlug(stem.to_string(), path.clone()));
            }
            if space.artifacts.len() + space.assets.len() >= MAX_ENTRIES {
                return Err(ScanError::TooManyEntries(space.artifacts.len() + space.assets.len() + 1));
            }
            ensure_within(&canon_root, &path)?;
            let raw = read_file_capped(&path, &mut total)?;
            let html = String::from_utf8(raw).map_err(|_| ScanError::NotUtf8(path.clone()))?;
            let title = resolve_title(&html).unwrap_or_else(|| stem.to_string());
            space
                .artifacts
                .insert(stem.to_string(), Artifact { html, title });
        }
        // Non-.html, non-manifest top-level files are ignored (assets live in assets/).
    }

    if total > MAX_SPACE_BYTES {
        return Err(ScanError::SpaceTooLarge(total));
    }

    finalize(&mut space);
    Ok(space)
}

/// Recursively scan the `assets/` subtree into `space.assets`, keyed by the
/// space-relative path (`assets/...`). Rejects symlinks and any path that
/// escapes the canonical root.
fn scan_assets(
    dir: &Path,
    canon_root: &Path,
    space: &mut Space,
    total: &mut u64,
) -> Result<(), ScanError> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(cur) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&cur)
            .map_err(|e| ScanError::Io(cur.clone(), e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ScanError::Io(cur.clone(), e))?;
        entries.sort_by_key(|e| e.file_name());
        for entry in &entries {
            let path = entry.path();
            let ftype = entry.file_type().map_err(|e| ScanError::Io(path.clone(), e))?;
            if ftype.is_symlink() {
                return Err(ScanError::Symlink(path));
            }
            if ftype.is_dir() {
                stack.push(path);
                continue;
            }
            if !ftype.is_file() {
                // FIFOs, sockets, devices — not servable content.
                return Err(ScanError::UnsupportedFileType(path));
            }
            if space.artifacts.len() + space.assets.len() >= MAX_ENTRIES {
                return Err(ScanError::TooManyEntries(space.artifacts.len() + space.assets.len() + 1));
            }
            ensure_within(canon_root, &path)?;
            let rel = rel_key(canon_root, &path)?;
            let bytes = read_file_capped(&path, total)?;
            space.assets.insert(
                rel,
                Asset {
                    content_type: mime_for(&path),
                    bytes,
                },
            );
        }
    }
    Ok(())
}

/// Compute the space-relative key (`assets/...`) for a file, validating every
/// segment against the asset-name grammar so a stored key can never itself
/// contain a traversal token.
fn rel_key(canon_root: &Path, path: &Path) -> Result<String, ScanError> {
    let canon = std::fs::canonicalize(path).map_err(|e| ScanError::Io(path.to_path_buf(), e))?;
    let rel = canon
        .strip_prefix(canon_root)
        .map_err(|_| ScanError::Escapes(path.to_path_buf()))?;
    let mut segs = Vec::new();
    for comp in rel.components() {
        match comp {
            Component::Normal(os) => {
                let s = os.to_str().ok_or_else(|| ScanError::BadAssetName(path.to_path_buf()))?;
                if !valid_asset_segment(s) {
                    return Err(ScanError::BadAssetName(path.to_path_buf()));
                }
                segs.push(s.to_string());
            }
            // Canonicalization removes `.`/`..`; anything else here is anomalous.
            _ => return Err(ScanError::BadAssetName(path.to_path_buf())),
        }
    }
    Ok(segs.join("/"))
}

/// Best-effort file length via `lstat` (0 if it can't be read — the subsequent
/// capped read is the real enforcement).
fn entry_len(path: &Path) -> u64 {
    std::fs::symlink_metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// Assert a path's canonical form is contained within the canonical root. Defends
/// against a symlink or `..` component slipping a file outside the space.
fn ensure_within(canon_root: &Path, path: &Path) -> Result<(), ScanError> {
    let canon = std::fs::canonicalize(path).map_err(|e| ScanError::Io(path.to_path_buf(), e))?;
    if canon.strip_prefix(canon_root).is_err() {
        return Err(ScanError::Escapes(path.to_path_buf()));
    }
    Ok(())
}

/// Read a file, enforcing the per-file cap on the **actual bytes read** (not a
/// pre-read `stat`, which a concurrent write could grow), accumulating the real
/// per-space total, and rejecting anything that isn't a regular file. Rejecting
/// non-regular files by `lstat` **before** opening is what keeps a FIFO named
/// `index.html` from blocking the scan forever (opening a FIFO blocks).
fn read_file_capped(path: &Path, total: &mut u64) -> Result<Vec<u8>, ScanError> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| ScanError::Io(path.to_path_buf(), e))?;
    if meta.file_type().is_symlink() {
        return Err(ScanError::Symlink(path.to_path_buf()));
    }
    if !meta.is_file() {
        return Err(ScanError::UnsupportedFileType(path.to_path_buf()));
    }
    // Read at most (per-file cap ∧ remaining per-space budget) + 1, then verify —
    // the `+1` lets us detect an over-limit file without trusting the stat length.
    let remaining = MAX_SPACE_BYTES.saturating_sub(*total);
    let limit = MAX_FILE_BYTES.min(remaining);
    let f = std::fs::File::open(path).map_err(|e| ScanError::Io(path.to_path_buf(), e))?;
    // Re-check via the opened descriptor (defends the common swap-after-lstat case).
    if !f.metadata().map_err(|e| ScanError::Io(path.to_path_buf(), e))?.is_file() {
        return Err(ScanError::UnsupportedFileType(path.to_path_buf()));
    }
    let mut buf = Vec::new();
    f.take(limit + 1)
        .read_to_end(&mut buf)
        .map_err(|e| ScanError::Io(path.to_path_buf(), e))?;
    let len = buf.len() as u64;
    if len > MAX_FILE_BYTES {
        return Err(ScanError::FileTooLarge(path.to_path_buf(), len));
    }
    *total = total.saturating_add(len);
    if *total > MAX_SPACE_BYTES {
        return Err(ScanError::SpaceTooLarge(*total));
    }
    Ok(buf)
}

/// After all files are read, compute nav order (manifest, else lexicographic)
/// and the home slug (`index` > `home` > first in nav order).
fn finalize(space: &mut Space) {
    // Manifest nav may have listed slugs; keep only ones that exist, **deduped**
    // (a manifest `nav: [a, a]` must not double the entry), then append any
    // remaining artifacts in lexicographic order so nothing is hidden. A `HashSet`
    // keeps this linear instead of O(artifacts × nav).
    let mut nav: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for s in &space.nav {
        if space.artifacts.contains_key(s) && seen.insert(s.clone()) {
            nav.push(s.clone());
        }
    }
    for slug in space.artifacts.keys() {
        if seen.insert(slug.clone()) {
            nav.push(slug.clone());
        }
    }
    space.nav = nav;

    space.home = if space.artifacts.contains_key("index") {
        Some("index".to_string())
    } else if space.artifacts.contains_key("home") {
        Some("home".to_string())
    } else {
        space.nav.first().cloned()
    };
}

/// Parse the optional `glasspad.yaml` — **structure only** (title, nav order).
/// Unknown keys are ignored; a syntactically invalid file is a hard error.
fn apply_manifest(text: &str, path: &Path, space: &mut Space) -> Result<(), ScanError> {
    #[derive(serde::Deserialize, Default)]
    struct Manifest {
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        nav: Vec<String>,
    }
    let m: Manifest =
        serde_yaml::from_str(text).map_err(|e| ScanError::Manifest(path.to_path_buf(), e.to_string()))?;
    if let Some(t) = m.title {
        let t = strip_unsafe_display_chars(&decode_entities(t.trim()));
        let t = t.trim();
        if !t.is_empty() {
            space.title = Some(bound_chars(t, MAX_TITLE_CHARS));
        }
    }
    // Record the requested nav order; `finalize` reconciles it against reality.
    space.nav = m.nav;
    Ok(())
}

/// The `.html` filename stem, or `None` for non-HTML files.
fn html_stem(name: &str) -> Option<&str> {
    for ext in [".html", ".htm"] {
        if let Some(stem) = name.strip_suffix(ext) {
            return Some(stem);
        }
    }
    None
}

/// Asset path-segment grammar. Mixed case allowed (agents name files freely);
/// `.`/`..`/empty are excluded because they never appear as `Normal` components.
fn valid_asset_segment(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// Validate a request-supplied asset sub-path (`{*path}` after `/{space}/assets/`).
/// Returns the space-relative key (`assets/...`) to look up in the pre-scanned map,
/// or `None` if any segment is malformed / a traversal token. The map lookup is
/// the real allowlist; this is defense-in-depth so a bad path can't even form a key.
pub fn asset_key_for_request(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let mut segs = vec![ASSETS_DIR.to_string()];
    for seg in path.split('/') {
        if !valid_asset_segment(seg) {
            return None;
        }
        segs.push(seg.to_string());
    }
    Some(segs.join("/"))
}

/// Extension → MIME allowlist. Unknown extensions fall back to
/// `application/octet-stream`; every asset response also carries `nosniff`, so a
/// wrong guess can never be upgraded to an executable type by the browser.
pub fn mime_for(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "eot" => "application/vnd.ms-fontobject",
        "wasm" => "application/wasm",
        "txt" | "md" => "text/plain; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        "pdf" => "application/pdf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
}

// --- title parsing (a tokenizer, not a regex) ------------------------------

/// Resolve an artifact title: `<title>` first, else the first `<h1>`. Parsed with
/// a small tag-aware scanner (case-insensitive, attribute-tolerant), entity-
/// decoded, whitespace-collapsed, and length-bounded. Returned `None` when
/// neither is present. The value is inserted downstream as **text**, never HTML.
pub fn resolve_title(html: &str) -> Option<String> {
    if let Some(t) = extract_element_text(html, "title") {
        return Some(t);
    }
    extract_element_text(html, "h1")
}

/// Extract the text content of the first `<tag>…</tag>`. Not a regex: it walks the
/// byte stream tracking tag boundaries so attributes (incl. a `>` inside a quoted
/// value), whitespace, case, HTML comments, and a look-alike close tag
/// (`</titlebar>`) don't fool it. (It does not yet skip `<script>`/`<style>`
/// raw-text bodies or distinguish SVG/MathML `<title>` — see the module notes;
/// the value is inserted as text, so those are correctness-only gaps.)
fn extract_element_text(html: &str, tag: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Skip HTML comments so `<!-- <title>x</title> -->` never matches. An
        // unterminated comment swallows the rest of the document → no title.
        // Compare on **bytes**, not `lower[i..]` (a str slice): `i` walks
        // byte-by-byte and can land inside a multi-byte char (a leading BOM,
        // an accented/emoji prefix before the first tag), where slicing a `str`
        // at that index panics. Byte-prefix comparison is boundary-safe, and
        // `-->`/tag names are ASCII so the match is identical.
        if bytes[i..].starts_with(b"<!--") {
            let end = lower[i + 4..].find("-->")?;
            i += 4 + end + 3;
            continue;
        }
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        // Try to match `<tag` followed by a delimiter (space, >, /, tab, newline).
        let after = i + 1;
        if lower[after..].starts_with(tag) {
            let j = after + tag.len();
            let delim = bytes.get(j).copied();
            if matches!(delim, Some(b' ') | Some(b'>') | Some(b'/') | Some(b'\t') | Some(b'\n') | Some(b'\r')) {
                let (content_start, self_closing) = end_of_open_tag(bytes, j)?;
                if self_closing {
                    return None; // `<title/>` has no text content
                }
                let content_end = find_close_tag(&lower, content_start, tag)?;
                let raw = &html[content_start..content_end];
                // Decode entities BEFORE collapsing whitespace, so `&nbsp;` folds;
                // then strip invisible/bidi chars that could reorder or spoof the
                // visible label (it lands in the trusted nav + `document.title`).
                let text = strip_unsafe_display_chars(&collapse_ws(decode_entities(&strip_tags(raw))))
                    .trim()
                    .to_string();
                if text.is_empty() {
                    return None;
                }
                return Some(bound_chars(&text, MAX_TITLE_CHARS));
            }
        }
        i = after;
    }
    None
}

/// Find the end of an opening tag, starting just after the tag name. Returns
/// `(content_start, self_closing)`. A `>` inside a quoted attribute value does
/// **not** terminate the tag.
fn end_of_open_tag(bytes: &[u8], from: usize) -> Option<(usize, bool)> {
    let mut i = from;
    let mut quote: Option<u8> = None;
    let mut last_non_ws = 0u8;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == q {
                    quote = None;
                }
            }
            None => match b {
                b'"' | b'\'' => quote = Some(b),
                b'>' => return Some((i + 1, last_non_ws == b'/')),
                _ => {}
            },
        }
        if !b.is_ascii_whitespace() {
            last_non_ws = b;
        }
        i += 1;
    }
    None
}

/// Find the matching `</tag>` at or after `start`, requiring the tag name be
/// followed by a real delimiter so `</titlebar>` doesn't close `</title>`.
/// Returns the byte index of the `<` of the close tag.
fn find_close_tag(lower: &str, start: usize, tag: &str) -> Option<usize> {
    let needle = format!("</{tag}");
    let bytes = lower.as_bytes();
    let mut from = start;
    while let Some(rel) = lower[from..].find(&needle) {
        let pos = from + rel;
        let after = pos + needle.len();
        match bytes.get(after) {
            Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') | Some(b'/') | None => {
                return Some(pos);
            }
            _ => from = after, // e.g. `</titlebar>` — keep looking
        }
    }
    None
}

/// Drop any nested tags inside the extracted content (e.g. `<h1><span>x</span>`).
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// Collapse runs of ASCII whitespace to single spaces.
fn collapse_ws(s: String) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_ws {
                out.push(' ');
            }
            prev_ws = true;
        } else {
            out.push(c);
            prev_ws = false;
        }
    }
    out
}

/// Remove characters that render invisibly or reorder surrounding text. A
/// resolved title is inserted into the **trusted** nav chrome (as `textContent`)
/// and set as `document.title`; it can never execute, but a bidi override
/// (`U+202E`) or zero-width run could reorder/spoof the visible label or the tab
/// title. These are stripped at resolution time so both the `<title>` and the nav
/// see a clean string. Ordinary whitespace controls are already folded to spaces
/// by `collapse_ws`; this targets the *non*-whitespace controls, the bidi
/// embeddings/overrides/isolates, and the zero-width/BOM marks.
fn strip_unsafe_display_chars(s: &str) -> String {
    s.chars().filter(|&c| !is_unsafe_display_char(c)).collect()
}

fn is_unsafe_display_char(c: char) -> bool {
    matches!(c,
        // C0 controls (non-whitespace) + DEL. Whitespace controls (U+0009..U+000D)
        // are left to collapse_ws; the rest have no place in a display label.
        '\u{0000}'..='\u{0008}' | '\u{000e}'..='\u{001f}' | '\u{007f}'
        // C1 controls.
        | '\u{0080}'..='\u{009f}'
        // Bidi embeddings / overrides / isolates (LRE/RLE/PDF/LRO/RLO, LRI..PDI).
        | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
        // Directional marks, zero-width space/non-joiner/joiner, word joiner, BOM.
        | '\u{200b}'..='\u{200f}' | '\u{2060}' | '\u{feff}'
    )
}

/// Bound a string to `max` chars (not bytes) without splitting a char.
fn bound_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect()
}

/// Decode the small set of HTML entities that realistically appear in titles.
/// Unknown entities are left verbatim (the value is rendered as text anyway).
fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'&' {
            // Push the full UTF-8 char starting at i.
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        if let Some(semi) = s[i..].find(';').filter(|&off| off <= 10) {
            let ent = &s[i + 1..i + semi];
            let decoded = match ent {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                "nbsp" => Some('\u{00a0}'),
                _ => decode_numeric(ent),
            };
            if let Some(c) = decoded {
                out.push(c);
                i += semi + 1;
                continue;
            }
        }
        out.push('&');
        i += 1;
    }
    out
}

fn decode_numeric(ent: &str) -> Option<char> {
    let num = ent.strip_prefix('#')?;
    let code = if let Some(hex) = num.strip_prefix(['x', 'X']) {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        num.parse::<u32>().ok()?
    };
    char::from_u32(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_prefers_title_tag_then_h1() {
        assert_eq!(
            resolve_title("<html><head><title>Sales Q3</title></head><body><h1>Other</h1>"),
            Some("Sales Q3".to_string())
        );
        assert_eq!(
            resolve_title("<body><h1>Just a Heading</h1></body>"),
            Some("Just a Heading".to_string())
        );
        assert_eq!(resolve_title("<p>no title here</p>"), None);
    }

    #[test]
    fn title_is_attribute_and_case_tolerant() {
        assert_eq!(
            resolve_title(r#"<TITLE lang="en">  Mixed  Case  </TITLE>"#),
            Some("Mixed Case".to_string())
        );
        // A tag that merely starts with the name must not match (`<titlebar>`).
        assert_eq!(resolve_title("<titlebar>x</titlebar><title>Real</title>"), Some("Real".to_string()));
    }

    #[test]
    fn title_decodes_entities_and_strips_nested_tags() {
        assert_eq!(
            resolve_title("<title>A &amp; B &lt;ok&gt;</title>"),
            Some("A & B <ok>".to_string())
        );
        assert_eq!(
            resolve_title("<h1>Hi <span class=x>there</span></h1>"),
            Some("Hi there".to_string())
        );
        assert_eq!(resolve_title("<title>caf&#233;</title>"), Some("café".to_string()));
    }

    #[test]
    fn title_skips_comments_and_lookalike_close_tags() {
        // A commented-out title must not win over the real one.
        assert_eq!(
            resolve_title("<!-- <title>Fake</title> --><title>Real</title>"),
            Some("Real".to_string())
        );
        // `</titlebar>` must not close `<title>`.
        assert_eq!(
            resolve_title("<title>Kept</title>bar text"),
            Some("Kept".to_string())
        );
        assert_eq!(
            resolve_title("<title>Real <x>y</x></title>"),
            Some("Real y".to_string())
        );
    }

    #[test]
    fn title_tolerates_gt_inside_quoted_attribute() {
        assert_eq!(
            resolve_title(r#"<title data-x="a>b">Real Title</title>"#),
            Some("Real Title".to_string())
        );
    }

    #[test]
    fn title_self_closing_has_no_text() {
        // `<title/>` yields nothing → fall through to h1.
        assert_eq!(
            resolve_title("<title/><h1>Heading</h1>"),
            Some("Heading".to_string())
        );
    }

    #[test]
    fn title_nbsp_entity_collapses() {
        assert_eq!(
            resolve_title("<title>a&nbsp;&nbsp;b</title>"),
            Some("a b".to_string())
        );
    }

    #[test]
    fn title_length_bounded() {
        let long = "x".repeat(500);
        let html = format!("<title>{long}</title>");
        assert_eq!(resolve_title(&html).unwrap().chars().count(), MAX_TITLE_CHARS);
    }

    #[test]
    fn title_injection_stays_inert_text() {
        // A hostile title is extracted as plain text; the shell inserts it via
        // textContent, so this can never become live markup.
        let t = resolve_title(r#"<title></title><script>alert(1)</script>"#);
        // Empty title element → falls through to h1 (none) → None.
        assert_eq!(t, None);
        // A nested tag inside the title is stripped; the surviving text is inert.
        let t2 = resolve_title(r#"<title>x"><img src=x onerror=alert(1)></title>"#).unwrap();
        assert!(!t2.contains("<img"));
        assert!(!t2.contains("onerror")); // the <img …> was stripped entirely
        assert_eq!(t2, "x\"");
    }

    #[test]
    fn title_tolerates_leading_bom_and_non_ascii_without_panicking() {
        // Regression: the byte-walking scanner must not slice a `str` at a byte
        // index inside a multi-byte char. A leading BOM (`create` / `serve` of a
        // BOM-prefixed artifact) and a non-ASCII text prefix before the first tag
        // both used to panic the server on scan.
        assert_eq!(
            resolve_title("\u{feff}<!doctype html><title>Home</title>"),
            Some("Home".to_string())
        );
        assert_eq!(
            resolve_title("café ☕ before any tag <h1>Heading</h1>"),
            Some("Heading".to_string())
        );
        // A BOM directly before a comment then a title (comment-skip path).
        assert_eq!(
            resolve_title("\u{feff}<!-- ☕ --><title>Real</title>"),
            Some("Real".to_string())
        );
        // Non-ASCII everywhere, no title/h1 → None, still no panic.
        assert_eq!(resolve_title("\u{feff}just café ☕ text, no tags"), None);
    }

    #[test]
    fn title_strips_bidi_and_zero_width_spoofing_chars() {
        // A bidi override / zero-width run in a title can reorder or hide the
        // visible label in the trusted nav; they must be stripped at resolution.
        assert_eq!(
            resolve_title("<title>Invoice \u{202e}txt.exe</title>"),
            Some("Invoice txt.exe".to_string())
        );
        assert_eq!(
            resolve_title("<title>a\u{200b}\u{200b}b\u{feff}c</title>"),
            Some("abc".to_string())
        );
        // A non-whitespace C0 control is removed, not rendered.
        assert_eq!(
            resolve_title("<title>ok\u{0007}now</title>"),
            Some("oknow".to_string())
        );
        // A title that is ONLY invisible chars collapses to nothing → None.
        assert_eq!(resolve_title("<title>\u{202e}\u{200b}\u{feff}</title>"), None);
    }

    #[test]
    fn asset_request_key_rejects_traversal() {
        assert_eq!(asset_key_for_request("data.json"), Some("assets/data.json".to_string()));
        assert_eq!(asset_key_for_request("sub/logo.svg"), Some("assets/sub/logo.svg".to_string()));
        assert_eq!(asset_key_for_request(""), None);
        assert_eq!(asset_key_for_request("../secret"), None);
        assert_eq!(asset_key_for_request("a/../../etc/passwd"), None);
        assert_eq!(asset_key_for_request("."), None);
        assert_eq!(asset_key_for_request("a//b"), None); // empty segment
        assert_eq!(asset_key_for_request("a/b/"), None);
        assert_eq!(asset_key_for_request("bad name.js"), None); // space
        assert_eq!(asset_key_for_request("weird\\path"), None); // backslash
    }

    #[test]
    fn mime_detection() {
        assert_eq!(mime_for(Path::new("a.css")), "text/css; charset=utf-8");
        assert_eq!(mime_for(Path::new("a.JS")), "text/javascript; charset=utf-8");
        assert_eq!(mime_for(Path::new("a.svg")), "image/svg+xml");
        assert_eq!(mime_for(Path::new("a.unknownext")), "application/octet-stream");
        assert_eq!(mime_for(Path::new("noext")), "application/octet-stream");
    }

    #[test]
    fn html_stem_only_matches_html() {
        assert_eq!(html_stem("index.html"), Some("index"));
        assert_eq!(html_stem("a.htm"), Some("a"));
        assert_eq!(html_stem("data.json"), None);
        assert_eq!(html_stem("noext"), None);
    }
}

// --- filesystem-level adversarial tests (scanner is the security surface) ---
#[cfg(test)]
mod fs_tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A throwaway temp directory, cleaned up on drop.
    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let p = std::env::temp_dir().join(format!("glasspad-space-{}-{}", std::process::id(), n));
            std::fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
        fn write(&self, rel: &str, contents: &[u8]) {
            let p = self.0.join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(p, contents).unwrap();
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn scans_artifacts_assets_and_resolves_home_title() {
        let d = TempDir::new();
        d.write("index.html", b"<title>Home</title><h1>hi</h1>");
        d.write("sales.html", b"<h1>Sales Q3</h1>");
        d.write("assets/data.json", b"{\"a\":1}");
        d.write("assets/sub/logo.svg", b"<svg></svg>");
        let space = scan_dir(d.path()).unwrap();

        assert_eq!(space.artifacts.len(), 2);
        assert_eq!(space.artifact("index").unwrap().title, "Home");
        assert_eq!(space.artifact("sales").unwrap().title, "Sales Q3");
        assert_eq!(space.home.as_deref(), Some("index"));
        assert_eq!(space.nav, vec!["index".to_string(), "sales".to_string()]);
        assert!(space.asset("assets/data.json").is_some());
        assert_eq!(space.asset("assets/sub/logo.svg").unwrap().content_type, "image/svg+xml");
    }

    #[test]
    fn reserved_slug_is_hard_error() {
        let d = TempDir::new();
        d.write("api.html", b"x"); // `api` is reserved
        assert!(matches!(scan_dir(d.path()), Err(ScanError::ReservedSlug(_, _))));
    }

    #[test]
    fn bad_slug_is_hard_error() {
        let d = TempDir::new();
        d.write("Bad Name.html", b"x");
        assert!(matches!(scan_dir(d.path()), Err(ScanError::BadSlug(_, _))));
    }

    #[test]
    fn duplicate_slug_across_html_extensions_is_hard_error() {
        let d = TempDir::new();
        d.write("page.html", b"x");
        d.write("page.htm", b"y"); // same stem → collision
        assert!(matches!(scan_dir(d.path()), Err(ScanError::DuplicateSlug(_, _))));
    }

    #[test]
    fn oversize_file_is_hard_error() {
        let d = TempDir::new();
        d.write("index.html", &vec![b'a'; (MAX_FILE_BYTES + 1) as usize]);
        assert!(matches!(scan_dir(d.path()), Err(ScanError::FileTooLarge(_, _))));
    }

    #[test]
    fn non_utf8_artifact_is_hard_error() {
        let d = TempDir::new();
        d.write("index.html", &[0xff, 0xfe, 0x00]);
        assert!(matches!(scan_dir(d.path()), Err(ScanError::NotUtf8(_))));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_artifact_is_rejected() {
        let d = TempDir::new();
        // A symlink whose target is a real secret outside the space.
        let secret = std::env::temp_dir().join(format!("glasspad-secret-{}", std::process::id()));
        std::fs::write(&secret, b"<h1>SECRET</h1>").unwrap();
        std::os::unix::fs::symlink(&secret, d.path().join("index.html")).unwrap();
        let r = scan_dir(d.path());
        let _ = std::fs::remove_file(&secret);
        assert!(matches!(r, Err(ScanError::Symlink(_))), "got {r:?}");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_asset_is_rejected() {
        let d = TempDir::new();
        d.write("index.html", b"<h1>ok</h1>");
        let secret = std::env::temp_dir().join(format!("glasspad-asset-secret-{}", std::process::id()));
        std::fs::write(&secret, b"leak").unwrap();
        std::fs::create_dir_all(d.path().join("assets")).unwrap();
        std::os::unix::fs::symlink(&secret, d.path().join("assets/leak.txt")).unwrap();
        let r = scan_dir(d.path());
        let _ = std::fs::remove_file(&secret);
        assert!(matches!(r, Err(ScanError::Symlink(_))), "got {r:?}");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_root_is_rejected() {
        let d = TempDir::new();
        d.write("real/index.html", b"<h1>ok</h1>");
        let link = std::env::temp_dir().join(format!("glasspad-rootlink-{}", std::process::id()));
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(d.path().join("real"), &link).unwrap();
        let r = scan_dir(&link);
        let _ = std::fs::remove_file(&link);
        assert!(matches!(r, Err(ScanError::Symlink(_))), "got {r:?}");
    }

    #[test]
    fn manifest_nav_orders_and_titles() {
        let d = TempDir::new();
        d.write("a.html", b"<h1>A</h1>");
        d.write("b.html", b"<h1>B</h1>");
        d.write("glasspad.yaml", b"title: My Space\nnav: [b, a]\n");
        let space = scan_dir(d.path()).unwrap();
        assert_eq!(space.title.as_deref(), Some("My Space"));
        assert_eq!(space.nav, vec!["b".to_string(), "a".to_string()]);
        // Home falls back to first-in-nav when there's no index/home.
        assert_eq!(space.home.as_deref(), Some("b"));
    }

    #[test]
    fn malformed_manifest_is_hard_error() {
        let d = TempDir::new();
        d.write("index.html", b"<h1>x</h1>");
        d.write("glasspad.yaml", b"title: [unterminated\n");
        assert!(matches!(scan_dir(d.path()), Err(ScanError::Manifest(_, _))));
    }

    #[test]
    fn oversized_manifest_is_hard_error() {
        let d = TempDir::new();
        d.write("index.html", b"<h1>x</h1>");
        // Valid YAML, but far over the tight manifest cap — must never reach the
        // parser (alias-bomb defense).
        let big = format!("title: {}\n", "a".repeat((MAX_MANIFEST_BYTES + 10) as usize));
        d.write("glasspad.yaml", big.as_bytes());
        assert!(matches!(scan_dir(d.path()), Err(ScanError::ManifestTooLarge(_, _))));
    }

    #[test]
    fn manifest_nav_dedupes_repeated_entries() {
        let d = TempDir::new();
        d.write("a.html", b"<h1>A</h1>");
        d.write("b.html", b"<h1>B</h1>");
        d.write("glasspad.yaml", b"nav: [a, a, b, a]\n");
        let space = scan_dir(d.path()).unwrap();
        assert_eq!(space.nav, vec!["a".to_string(), "b".to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn fifo_artifact_is_rejected_without_hanging() {
        use std::os::unix::net::UnixListener;
        let d = TempDir::new();
        d.write("index.html", b"<h1>ok</h1>");
        // A unix socket is a non-regular file; a FIFO would block on open, which is
        // exactly why we reject via lstat first. A socket exercises the same guard
        // without the test needing mkfifo.
        let _sock = UnixListener::bind(d.path().join("weird.html")).unwrap();
        assert!(matches!(scan_dir(d.path()), Err(ScanError::UnsupportedFileType(_))));
    }
}
