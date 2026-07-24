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

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use super::{valid_name, RESERVED};

/// Per-file byte ceiling. A single artifact/asset larger than this is a hard
/// error — a local dev tool has no business streaming huge blobs, and an
/// unbounded read is a trivial memory-exhaustion foot-gun.
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024; // 8 MiB
/// Per-space aggregate ceiling across every artifact + asset.
pub const MAX_SPACE_BYTES: u64 = 64 * 1024 * 1024; // 64 MiB
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
            let raw = read_file_capped(&path, &mut total)?;
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
                return Err(ScanError::BadAssetName(path));
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

/// Assert a path's canonical form is contained within the canonical root. Defends
/// against a symlink or `..` component slipping a file outside the space.
fn ensure_within(canon_root: &Path, path: &Path) -> Result<(), ScanError> {
    let canon = std::fs::canonicalize(path).map_err(|e| ScanError::Io(path.to_path_buf(), e))?;
    if canon.strip_prefix(canon_root).is_err() {
        return Err(ScanError::Escapes(path.to_path_buf()));
    }
    Ok(())
}

/// Read a file, enforcing the per-file cap and accumulating the per-space total.
fn read_file_capped(path: &Path, total: &mut u64) -> Result<Vec<u8>, ScanError> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| ScanError::Io(path.to_path_buf(), e))?;
    if meta.file_type().is_symlink() {
        return Err(ScanError::Symlink(path.to_path_buf()));
    }
    let len = meta.len();
    if len > MAX_FILE_BYTES {
        return Err(ScanError::FileTooLarge(path.to_path_buf(), len));
    }
    *total = total.saturating_add(len);
    if *total > MAX_SPACE_BYTES {
        return Err(ScanError::SpaceTooLarge(*total));
    }
    std::fs::read(path).map_err(|e| ScanError::Io(path.to_path_buf(), e))
}

/// After all files are read, compute nav order (manifest, else lexicographic)
/// and the home slug (`index` > `home` > first in nav order).
fn finalize(space: &mut Space) {
    // Manifest nav may have listed slugs; keep only ones that exist, then append
    // any remaining artifacts in lexicographic order so nothing is hidden.
    let mut nav: Vec<String> = space
        .nav
        .iter()
        .filter(|s| space.artifacts.contains_key(*s))
        .cloned()
        .collect();
    for slug in space.artifacts.keys() {
        if !nav.contains(slug) {
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
        let t = decode_entities(t.trim());
        if !t.is_empty() {
            space.title = Some(bound_chars(&t, MAX_TITLE_CHARS));
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
/// byte stream tracking tag boundaries so attributes, whitespace, and case in the
/// opening tag don't fool it.
fn extract_element_text(html: &str, tag: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
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
                // Find the end of the opening tag.
                if let Some(gt) = lower[j..].find('>') {
                    let content_start = j + gt + 1;
                    // Self-closing opening tag has no text content.
                    if bytes.get(j + gt - 1) == Some(&b'/') {
                        return None;
                    }
                    let close = format!("</{tag}");
                    if let Some(rel) = lower[content_start..].find(&close) {
                        let raw = &html[content_start..content_start + rel];
                        let text = decode_entities(&collapse_ws(strip_tags(raw)));
                        let text = text.trim().to_string();
                        if text.is_empty() {
                            return None;
                        }
                        return Some(bound_chars(&text, MAX_TITLE_CHARS));
                    }
                    return None;
                }
                return None;
            }
        }
        i = after;
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
}
