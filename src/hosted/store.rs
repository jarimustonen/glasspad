//! Immutable page storage + retention/GC for the hosted share server.
//!
//! Each published page is a **single-artifact space** (exactly what
//! `create`/`render` produce) persisted under `<root>/pages/<slug>/`:
//!
//! ```text
//! <root>/pages/<slug>/artifact.html   the raw artifact body (fragment or full doc)
//! <root>/pages/<slug>/meta.json       { schema, slug, tenant, title, created_at }
//! ```
//!
//! The published **baseline** is immutable: `artifact.html` is written once (to a
//! temp path then atomically `rename`d in, so a crash never exposes a half-page) and
//! **never rewritten**, and `publish` mints a **fresh random slug** for every new
//! page, so one tenant can never overwrite another's page. The in-memory [`Snapshot`]
//! the read handlers serve is the serving source of truth; the on-disk tree is the
//! persistence + GC source of truth.
//!
//! **B2 multi-round live overlay.** A page's *served* body can advance round by round
//! ([`Store::push_round`], owner-scoped) without touching the immutable baseline: a
//! re-render is persisted as the `live.html` + `live.json` overlay sidecars and the
//! served snapshot body is swapped in place. `load_page` serves the overlay when a
//! valid one is present (else the baseline), so the current round survives restart and
//! the hourly GC-rescan; page retention GC reaps the whole directory (baseline +
//! overlay) together, so the live "session" is bounded by the same retention window.
//!
//! Retention/GC removes page directories older than the retention window and swaps
//! a rebuilt snapshot so an expired page is promptly **no longer served**.
//!
//! **Idempotency keys** (optional). A publish may carry an `idempotency_key`. When
//! present, the store durably records `key → slug` — under a **per-tenant**
//! directory `<root>/idem/<tenant>/<sha256(key)>.json`, written (fsync + atomic
//! rename) **after** the page files are durable — and a later publish with the same
//! key returns the *same* page instead of minting a new one. This gives an API-key
//! publisher exactly-once semantics across a lost receipt. A key whose page has
//! since been GC'd/deleted ("dangling") falls through to a fresh create, and GC
//! reclaims the now-dead mapping so the idem tree stays bounded to live pages.
//! Isolation is layered: the per-tenant directory scopes which mapping is read, the
//! record records its owning tenant, and the mapped page's own `meta.json` must
//! record the same tenant — so a misplaced/hand-edited mapping can never hand one
//! tenant another tenant's page. No key → a fresh slug every time (the default path
//! is unchanged).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::artifact_host::ArtifactHost;
use crate::artifact_host::space::{
    self, Artifact, BundleAsset, BundlePage, Snapshot, Space, build_space_bundle,
};
use crate::artifact_host::valid_space;
use crate::server::SINGLE_SLUG;

use super::slug;

/// Hard ceiling on total stored pages — a backstop against unbounded disk/memory
/// growth from ingest (per-tenant quotas are future work; this is the global cap).
pub const MAX_PAGES: usize = 100_000;

/// On-disk metadata sidecar for one page. `schema` guards the format; `tenant` is
/// the authenticated owner (set server-side, never from the request body).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageMeta {
    pub schema: u32,
    pub slug: String,
    pub tenant: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
}

const META_SCHEMA: u32 = 1;
const ARTIFACT_FILE: &str = "artifact.html";
const META_FILE: &str = "meta.json";

/// On-disk metadata sidecar for one multi-artifact **space** (Gap 1). Records the
/// authenticated owner + the structural nav/home/title so a reload reconstructs the
/// same `Space` the producer published. `created_at` fixes the retention window;
/// `updated_at` advances on each in-place re-publish (it does **not** reset the
/// retention clock — a live docsite stays reachable for the whole window from first
/// publish, refreshed in place).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceMeta {
    pub schema: u32,
    pub slug: String,
    pub tenant: String,
    pub title: Option<String>,
    pub nav: Vec<String>,
    /// Optional grouped, one-level-nestable nav (`glasspad.yaml`'s `groups:`),
    /// persisted so a reload reconstructs the grouped sidebar + landing. `#[serde(
    /// default)]` keeps the schema backward-compatible: a `meta.json` written before
    /// this field existed deserializes with an empty vec (the flat-nav fallback).
    /// Re-validated on reload through [`space::build_space_bundle`].
    #[serde(default)]
    pub nav_groups: Vec<space::NavGroup>,
    pub home: Option<String>,
    /// Optional emoji favicon for the space's outer shell document. `#[serde(default)]`
    /// keeps the schema backward-compatible: a `meta.json` written before this field
    /// existed deserializes with `favicon: None` (the built-in default renders). Always
    /// a value already validated at ingest ([`crate::favicon::validate`]).
    #[serde(default)]
    pub favicon: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const SPACE_META_SCHEMA: u32 = 1;
/// The subdirectory of a `spaces/<slug>/` dir holding the per-page artifact bodies
/// (`artifacts/<page>.html`). Kept separate from the page's flat `artifact.html`.
const SPACE_ARTIFACTS_DIR: &str = "artifacts";
/// The subdirectory holding a space's static assets, mirroring the scanned asset
/// keys (`assets/<rel...>`).
const SPACE_ASSETS_SUBDIR: &str = "assets";

/// B2 multi-round **live overlay** sidecars (a page's *current round* of content).
/// The immutable baseline `artifact.html` is **never** rewritten — a multi-round
/// re-render is stored here instead, so the published record's immutability is
/// preserved while the *served* body advances round by round. When present + valid,
/// `live.html` is the served body (else the baseline); `live.json` records the
/// monotonic round number so the shell's SSE swap carries a `Last-Event-ID` cursor.
const LIVE_FILE: &str = "live.html";
const LIVE_META_FILE: &str = "live.json";
const LIVE_SCHEMA: u32 = 1;

/// On-disk sidecar for a page's live round. `round` is monotonic (starts at 1 on the
/// first push; the immutable baseline is round 0). Written **after** `live.html` is
/// durable; `content_version` records the digest of the body it was written for, so a
/// crash-torn pair (a new `live.html` with a stale `live.json`, or vice versa) is
/// **detected on load** — the body's recomputed digest won't match — and the overlay
/// is ignored (the page reverts to the immutable baseline) rather than serving a body
/// under the wrong round label.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LiveMeta {
    schema: u32,
    round: u64,
    content_version: String,
    updated_at: DateTime<Utc>,
}

/// Schema guard for the on-disk idempotency-mapping sidecar.
const IDEM_SCHEMA: u32 = 1;

/// One durable `key → slug` idempotency mapping. Stored one-per-file at
/// `<root>/idem/<tenant>/<sha256(key)>.json`. `schema` guards the format; `slug`
/// is validated against the space grammar on read; `tenant` records the owner so a
/// mapping read for the wrong tenant (a misplaced/restored file) is rejected rather
/// than honored (defense-in-depth on top of the per-tenant directory scoping).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdemRecord {
    schema: u32,
    tenant: String,
    slug: String,
}

/// The persistent page store. Holds the storage root and a handle to the
/// [`ArtifactHost`] whose snapshot it keeps in sync on publish + GC.
pub struct Store {
    pages_dir: PathBuf,
    /// Root of the multi-artifact **space** tree (`<root>/spaces/<slug>/`), the
    /// Gap-1 hosted-space storage. Parallel to `pages_dir` so the single-file page
    /// path is untouched; both feed the one in-memory snapshot.
    spaces_dir: PathBuf,
    /// Root of the per-tenant idempotency-mapping tree (`<root>/idem/<tenant>/`).
    /// A tenant's mappings live only under its own subdirectory, so a key lookup
    /// is scoped to the authenticated tenant by construction.
    idem_dir: PathBuf,
    /// Root of the per-tenant **stable space-key** mapping tree
    /// (`<root>/space-idem/<tenant>/`). A `--space-key` maps to a space slug so a
    /// re-publish updates that space **in place** (owner-scoped, per-tenant scoped).
    space_idem_dir: PathBuf,
    host: Arc<ArtifactHost>,
    /// Serializes the read-snapshot → mutate-disk → swap-snapshot critical section
    /// across `publish` and `gc`. Without it, two concurrent publishes (or a
    /// publish racing GC) each read the same snapshot, clone it, and swap — the
    /// last swap wins and silently drops the other's page from the served set
    /// (it stays on disk but 404s until restart). The lock makes each mutation
    /// read the *latest* snapshot, so no update is lost and GC can't resurrect a
    /// page it just deleted.
    mutation: std::sync::Mutex<()>,
}

/// Outcome of a successful publish.
#[derive(Debug)]
pub struct Published {
    pub slug: String,
    pub title: String,
    /// `true` when this call minted a new page; `false` when an idempotency key
    /// replayed an already-published page. The ingest handler maps this to `201`
    /// vs `200`.
    pub created: bool,
}

/// One page of a published space, for the ingest `--json` envelope (nav order).
#[derive(Debug, Clone)]
pub struct PublishedPage {
    pub slug: String,
    pub title: String,
}

/// Outcome of a successful [`Store::publish_space`].
#[derive(Debug)]
pub struct PublishedSpace {
    /// The space capability slug (`/p/<slug>/`).
    pub slug: String,
    /// Resolved space title (from the producer manifest), if any.
    pub title: Option<String>,
    /// The home slug (`index` > `home` > first in nav order).
    pub home: Option<String>,
    /// Pages in nav order, with resolved titles.
    pub pages: Vec<PublishedPage>,
    /// `true` when a fresh slug was minted; `false` when a stable `--space-key`
    /// updated an existing space in place.
    pub created: bool,
}

/// A running budget bounding what [`Store::load_space`] pulls off disk before
/// [`space::build_space_bundle`] gets a chance to reject an oversize space. Per-file
/// reads are already capped; this bounds the **aggregate** bytes and **entry count**
/// across a space's pages + assets so a hand-tampered store cannot OOM the process at
/// startup / GC-rescan by planting many near-cap files.
struct LoadBudget {
    entries: usize,
    bytes: u64,
}

impl LoadBudget {
    fn new() -> Self {
        LoadBudget {
            entries: 0,
            bytes: 0,
        }
    }

    /// Count one entry; `false` once past [`space::MAX_ENTRIES`].
    fn reserve_entry(&mut self) -> bool {
        self.entries += 1;
        self.entries <= space::MAX_ENTRIES
    }

    /// Add `n` bytes; `false` on overflow or once past [`space::MAX_SPACE_BYTES`].
    fn add_bytes(&mut self, n: u64) -> bool {
        match self.bytes.checked_add(n) {
            Some(t) => {
                self.bytes = t;
                t <= space::MAX_SPACE_BYTES
            }
            None => false,
        }
    }
}

impl Store {
    /// Open (creating if needed) the store rooted at `root`, load every existing
    /// page into the host's initial snapshot, and return the store. A corrupt or
    /// oversize individual page is **skipped with a log line**, not fatal — one bad
    /// directory must not stop the whole server coming up. Returns the loaded page
    /// count via [`Store::page_count`] after open.
    pub fn open(root: &Path, host: Arc<ArtifactHost>) -> std::io::Result<Self> {
        let pages_dir = root.join("pages");
        std::fs::create_dir_all(&pages_dir)?;
        let spaces_dir = root.join("spaces");
        std::fs::create_dir_all(&spaces_dir)?;
        let idem_dir = root.join("idem");
        std::fs::create_dir_all(&idem_dir)?;
        let space_idem_dir = root.join("space-idem");
        std::fs::create_dir_all(&space_idem_dir)?;
        // Persist the `pages`/`spaces`/`idem`/`space-idem` directory entries
        // themselves before any write relies on them being present after a crash
        // (fsync of a child dir does not persist the child's entry in its parent).
        fsync_dir(root)?;
        let store = Store {
            pages_dir,
            spaces_dir,
            idem_dir,
            space_idem_dir,
            host,
            mutation: std::sync::Mutex::new(()),
        };
        // Recover any space directory left mid-replace by a crash BEFORE scanning, so
        // an interrupted in-place update never disappears (its only copy may be in a
        // `.<slug>.old` backup that GC would otherwise reap).
        if let Err(e) = store.recover_space_staging() {
            eprintln!("glasspad host: space staging recovery failed: {e}");
        }
        let snap = store.scan_disk();
        store.host.swap(snap);
        Ok(store)
    }

    /// Reconcile the `spaces/` tree's crash-staging directories (`materialize_space`'s
    /// two-phase replace). For a `.<slug>.old` backup: if the live `spaces/<slug>` is
    /// **missing** (a crash between the two renames left the space only in the backup),
    /// restore it (`rename(.old → final)`) — never lose the last committed generation;
    /// otherwise the backup is a completed-replace remnant and is reclaimed. A
    /// `.<slug>.tmp` is an incomplete stage and is always dropped. Idempotent, so it is
    /// safe to run at startup and inside GC. Fsyncs `spaces/` when it restored anything.
    fn recover_space_staging(&self) -> std::io::Result<()> {
        let rd = match std::fs::read_dir(&self.spaces_dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        let mut restored = false;
        for entry in rd.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let path = entry.path();
            if let Some(slug) = name.strip_prefix('.').and_then(|s| s.strip_suffix(".old")) {
                let final_dir = self.spaces_dir.join(slug);
                if final_dir.exists() {
                    // The replace completed; the backup is dead weight.
                    let _ = std::fs::remove_dir_all(&path);
                } else {
                    // The replace was interrupted; the backup IS the live space.
                    std::fs::rename(&path, &final_dir)?;
                    restored = true;
                }
            } else if name.starts_with('.') {
                // A `.tmp` stage (or any other dotfile) is an incomplete write remnant.
                let _ = std::fs::remove_dir_all(&path);
            }
        }
        if restored {
            fsync_dir(&self.spaces_dir)?;
        }
        Ok(())
    }

    /// Acquire the mutation lock, recovering the guard if a previous holder panicked
    /// (poisoning it). The only state guarded is on-disk plus a fresh snapshot read
    /// on each entry, so a poisoned lock is safe to re-take — recovering keeps one
    /// panic from permanently bricking all publishes/GC.
    fn lock_mutation(&self) -> std::sync::MutexGuard<'_, ()> {
        self.mutation.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The number of pages currently served (snapshot space count).
    pub fn page_count(&self) -> usize {
        self.host.snapshot().spaces.len()
    }

    /// The authenticated owner tenant recorded in `slug`'s `meta.json`, or `None`
    /// if the slug is absent/unreadable or its meta names a different slug. This is
    /// the authority the return-channel scoping uses: the submit handler binds a
    /// submission to this owner, and a tenant may read a slug's submissions only
    /// when this equals its authenticated id. (`push_round` is *not* routed through
    /// here — it uses the page-only `page_owned_by`, so multi-round stays a
    /// single-artifact-page feature; see the return-channel notes for the space
    /// follow-up.)
    ///
    /// A slug can name **either** a single-artifact page (`pages/`) **or** a
    /// multi-artifact space (`spaces/`) — the two share one slug keyspace and the
    /// shell's hosted return-channel endpoint (`/api/v1/pages/<slug>/submit`) is the
    /// same for both — so this checks the page tree first, then the space tree.
    /// Without the space fallback, every CLI-published page (the CLI always uploads a
    /// *space* bundle, even for a single file) would be an owner-less `404` on submit
    /// and on the owner's read — the return channel would be dead for exactly the
    /// pages the CLI produces.
    pub fn page_tenant(&self, slug: &str) -> Option<String> {
        let page_owner = self.owner_from_meta::<PageMeta, _>(
            &self.pages_dir.join(slug).join(META_FILE),
            slug,
            |m| (m.schema == META_SCHEMA).then_some((m.slug, m.tenant)),
        );
        let space_owner = self.owner_from_meta::<SpaceMeta, _>(
            &self.spaces_dir.join(slug).join(META_FILE),
            slug,
            |m| (m.schema == SPACE_META_SCHEMA).then_some((m.slug, m.tenant)),
        );
        // Fail closed under ambiguity: `fresh_slug` keeps the page and space trees
        // from ever sharing a slug, so a slug present in BOTH is corrupt/tampered
        // state where the two metas could name different owners and the served
        // snapshot (pages win) could disagree with a naive pages-first pick. Rather
        // than risk binding a submission to the wrong tenant, refuse — the caller
        // maps `None` to the same opaque 404 as a missing page, leaking nothing.
        match (page_owner, space_owner) {
            (Some(_), Some(_)) => None,
            (Some(t), None) | (None, Some(t)) => Some(t),
            (None, None) => None,
        }
    }

    /// Read + validate an owner tenant from a `meta.json` sidecar. Bounded read;
    /// `extract` returns `(meta.slug, meta.tenant)` only when the schema is current.
    /// The recorded slug must equal the addressed `slug` and the tenant must satisfy
    /// the space grammar (matching the loaders' defense-in-depth), else `None` — a
    /// hand-tampered meta cannot smuggle a bad owner into route authorization.
    fn owner_from_meta<T, F>(&self, meta_path: &Path, slug: &str, extract: F) -> Option<String>
    where
        T: serde::de::DeserializeOwned,
        F: FnOnce(T) -> Option<(String, String)>,
    {
        let bytes = read_capped(meta_path, MAX_META_BYTES)
            .ok()
            .filter(|b| b.len() as u64 <= MAX_META_BYTES)?;
        let (meta_slug, tenant) = extract(serde_json::from_slice::<T>(&bytes).ok()?)?;
        (meta_slug == slug && valid_space(&tenant)).then_some(tenant)
    }

    /// The currently-served artifact body a return-channel submission answered, or
    /// `None` if the slug is not served. Used to compute the authoritative
    /// content-version server-side (never from the payload).
    ///
    /// The hosted submit endpoint is **space-level** (`/api/v1/pages/<space>/submit`),
    /// but a multi-page space's form lives on a *specific* page, and the trusted shell
    /// reports which one in its envelope (`slug: <current>`). `page` is that reported
    /// artifact slug: when it names a real artifact of this space, its body is the one
    /// the submission answered (so a non-home page's `content_version` is checked
    /// against the right body, not the home's — otherwise every non-home form would
    /// spuriously 409). `page` is untrusted, so a missing/empty/unknown value falls
    /// back to the lone single-artifact body ([`SINGLE_SLUG`] = `index`, the single-
    /// file / single-page case) and then the space **home** (`index` > `home` > first
    /// in nav) — never an escape from this space.
    pub fn page_body(&self, space_slug: &str, page: Option<&str>) -> Option<String> {
        let snap = self.host.snapshot();
        let space = snap.space(space_slug)?;
        if let Some(art) = page
            .filter(|p| !p.is_empty())
            .and_then(|p| space.artifact(p))
        {
            return Some(art.html.clone());
        }
        if let Some(art) = space.artifact(SINGLE_SLUG) {
            return Some(art.html.clone());
        }
        let home = space.home.as_deref()?;
        space.artifact(home).map(|a| a.html.clone())
    }

    /// Scan the whole store (`pages/` **and** `spaces/`) into a fresh [`Snapshot`].
    /// Pages and spaces share the one slug keyspace (a page is a single-artifact
    /// space in the snapshot); a slug can never collide across the two trees because
    /// [`Store::fresh_slug`] checks both plus the live snapshot at mint time.
    fn scan_disk(&self) -> Snapshot {
        let mut snap = Snapshot::empty();
        self.scan_pages_into(&mut snap);
        self.scan_spaces_into(&mut snap);
        snap
    }

    /// Scan the `pages/` tree into `snap`. Each subdirectory is one single-artifact
    /// page; unreadable/corrupt/oversize/invalid pages are skipped + logged.
    fn scan_pages_into(&self, snap: &mut Snapshot) {
        let rd = match std::fs::read_dir(&self.pages_dir) {
            Ok(rd) => rd,
            Err(e) => {
                eprintln!(
                    "glasspad host: cannot read pages dir {}: {e}",
                    self.pages_dir.display()
                );
                return;
            }
        };
        for entry in rd.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            match self.load_page(&dir) {
                Ok(Some((meta, space))) => {
                    snap.spaces.insert(meta.slug.clone(), space);
                }
                Ok(None) => {}
                Err(e) => eprintln!(
                    "glasspad host: skipping unreadable page {}: {e}",
                    dir.display()
                ),
            }
        }
    }

    /// Load one page directory into `(meta, space)`. Validates the slug grammar and
    /// that the on-disk slug matches the directory name (a mismatched/hostile dir
    /// name is skipped). Bounded read of the artifact body (same per-file cap).
    fn load_page(&self, dir: &Path) -> std::io::Result<Option<(PageMeta, Space)>> {
        // Reject a symlinked page directory (defense-in-depth: a symlink under the
        // store could point the loader at files outside the tree). `symlink_metadata`
        // does not follow the link.
        if std::fs::symlink_metadata(dir)?.file_type().is_symlink() {
            eprintln!(
                "glasspad host: page dir {} is a symlink; skipping",
                dir.display()
            );
            return Ok(None);
        }
        let meta_path = dir.join(META_FILE);
        let art_path = dir.join(ARTIFACT_FILE);
        if !meta_path.is_file() || !art_path.is_file() {
            return Ok(None);
        }
        // Bounded read: a hand-tampered/oversize meta.json must not force an
        // unbounded allocation on startup or hourly GC-rescan.
        let meta_bytes = read_capped(&meta_path, MAX_META_BYTES)?;
        if meta_bytes.len() as u64 > MAX_META_BYTES {
            eprintln!(
                "glasspad host: meta.json in {} exceeds {MAX_META_BYTES} bytes; skipping",
                dir.display()
            );
            return Ok(None);
        }
        let meta: PageMeta = match serde_json::from_slice(&meta_bytes) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("glasspad host: bad meta.json in {}: {e}", dir.display());
                return Ok(None);
            }
        };
        // Defense-in-depth: validate every reloaded field so a hand-edited store
        // can't smuggle a bad schema/slug/tenant/title into the router. The
        // directory name is the authority for the served slug.
        let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if meta.schema != META_SCHEMA
            || !valid_space(&meta.slug)
            || meta.slug != dir_name
            || !valid_space(&meta.tenant)
            || meta.title.chars().count() > space::MAX_TITLE_CHARS
        {
            eprintln!(
                "glasspad host: page dir {} has invalid meta (schema/slug/tenant/title); skipping",
                dir.display()
            );
            return Ok(None);
        }
        // Serve the live round overlay when a valid one is present (B2 multi-round);
        // else the immutable baseline. The overlay never widens the boundary — it is
        // a body swap only; the title stays the baseline's.
        let baseline = read_capped_utf8(&art_path)?;
        let html = self.load_live_body(dir).unwrap_or(baseline);
        Ok(Some((
            meta.clone(),
            one_artifact_space(html, meta.title.clone()),
        )))
    }

    /// Read a page directory's live overlay, if a **consistent** one is present:
    /// `live.json` parses with the right schema, `live.html` reads within the per-file
    /// cap, **and** the body's recomputed content-version matches the meta's — so a
    /// crash-torn pair (body and meta from different rounds) is rejected. Any
    /// absence/corruption/mismatch returns `None`, so the caller reverts to the
    /// immutable baseline; a half-written or hand-tampered overlay can never blank a
    /// page or serve a body under the wrong round label.
    fn read_live_overlay(&self, dir: &Path) -> Option<(LiveMeta, String)> {
        let lm_bytes = read_capped(&dir.join(LIVE_META_FILE), MAX_META_BYTES).ok()?;
        if lm_bytes.len() as u64 > MAX_META_BYTES {
            return None;
        }
        let lm: LiveMeta = serde_json::from_slice(&lm_bytes).ok()?;
        if lm.schema != LIVE_SCHEMA {
            return None;
        }
        let body = read_capped_utf8(&dir.join(LIVE_FILE)).ok()?;
        // Consistency gate: the meta must describe THIS body (else a crash between the
        // two renames left a mismatched pair — ignore it, revert to baseline).
        if crate::submissions::content_version(&body) != lm.content_version {
            return None;
        }
        Some((lm, body))
    }

    /// The live round body for a page directory, if a valid, self-consistent overlay
    /// is present; else `None` (the immutable baseline is served).
    fn load_live_body(&self, dir: &Path) -> Option<String> {
        self.read_live_overlay(dir).map(|(_, body)| body)
    }

    /// The current live round number for a page directory (0 when there is no valid,
    /// self-consistent overlay — i.e. the immutable baseline is what is served). A
    /// torn/corrupt overlay reads as 0 so the next push re-derives round 1 over the
    /// baseline it also reverts to — round and served body stay consistent.
    fn read_live_round(&self, dir: &Path) -> u64 {
        self.read_live_overlay(dir)
            .map(|(lm, _)| lm.round)
            .unwrap_or(0)
    }

    /// Publish one immutable page for `tenant`. Generates a fresh unguessable slug
    /// (regenerating on the astronomically-unlikely collision), writes the artifact
    /// and meta atomically, and inserts the new single-artifact space into the live
    /// snapshot via the host's atomic swap. Returns the minted slug and resolved
    /// title. Enforces [`MAX_PAGES`].
    ///
    /// When `idempotency_key` is `Some`, the publish is exactly-once for that
    /// (tenant, key): a repeat with a key whose page is still served returns that
    /// same page rather than minting a new one, and the durable `key → slug` mapping
    /// is written **after** the page is durable (see [`Store::write_page`] and
    /// [`Store::write_idem`]) so a crash never leaves a mapping pointing at a page
    /// that isn't on disk. A dangling key (mapped page GC'd/deleted) falls through
    /// to a fresh create. `None` → a fresh slug every time.
    pub fn publish(
        &self,
        tenant: &str,
        html: String,
        title_override: Option<String>,
        idempotency_key: Option<&str>,
    ) -> Result<Published, PublishError> {
        // Serialize the whole lookup→write→swap window so concurrent publishes (and
        // GC) can't lose each other's snapshot updates, and so two concurrent
        // same-key publishes can't both mint a page (the second sees the first's
        // mapping). Held across the blocking disk write; ingest calls this on a
        // blocking thread (`spawn_blocking`).
        let _guard = self.lock_mutation();
        let current = self.host.snapshot();

        // Idempotency fast-path: a recorded key whose page is still served returns
        // it (checked before the capacity gate — returning an existing page must not
        // be blocked by a full store).
        if let Some(key) = idempotency_key
            && let Some(existing) = self.lookup_idempotent(tenant, key, &current)?
        {
            return Ok(existing);
        }

        if current.spaces.len() >= MAX_PAGES {
            return Err(PublishError::Full);
        }

        // Mint a slug not already present on disk or in the snapshot.
        let slug = self.fresh_slug(&current)?;

        let title = title_override
            .filter(|t| !t.trim().is_empty())
            .or_else(|| space::resolve_title(&html))
            .unwrap_or_else(|| slug.clone());

        let meta = PageMeta {
            schema: META_SCHEMA,
            slug: slug.clone(),
            tenant: tenant.to_string(),
            title: title.clone(),
            created_at: Utc::now(),
        };

        self.write_page(&slug, &html, &meta)
            .map_err(PublishError::Io)?;

        // Record the durable key → slug mapping only AFTER the page is on disk and
        // fsync'd. Ordering is the crash-safety contract: if we crash between the two
        // writes, at worst an orphan page exists with no mapping — the caller retries
        // and we create fresh (safe). We never persist a mapping to a page that isn't
        // durable, so a key can never resolve to a missing/half-written page.
        if let Some(key) = idempotency_key {
            self.write_idem(tenant, key, &slug)
                .map_err(PublishError::Io)?;
        }

        // Clone-modify-swap: readers in flight keep the old Arc; new readers see the
        // added page. (O(n) rebuild is acceptable at this iteration's scale; a
        // future Arc-shared Space body would make it O(1) — see plan §6.)
        let mut spaces = current.spaces.clone();
        spaces.insert(slug.clone(), one_artifact_space(html, title.clone()));
        self.host.swap(Snapshot { spaces });

        Ok(Published {
            slug,
            title,
            created: true,
        })
    }

    /// Push a new **live round** of content for `slug`, advancing the B2 multi-round
    /// exchange. Owner-scoped: the page's own `meta.json` must record `tenant` as its
    /// owner, else this is an opaque `NoSuchPage` (a tenant learns nothing about, and
    /// can never re-render, another tenant's page). The immutable baseline
    /// `artifact.html` is untouched; the new body is persisted as the `live.html` /
    /// `live.json` overlay (durable, so it survives restart + hourly GC-rescan), the
    /// **served snapshot body is swapped in place** (title unchanged), and connected
    /// shells are pushed a keyed `round` event over the live-reload SSE carrier.
    /// Returns the new monotonic round number and the body's content-version (the same
    /// value a submission for this round must echo — [`crate::submissions::content_version`]).
    pub fn push_round(
        &self,
        tenant: &str,
        slug: &str,
        html: String,
    ) -> Result<RoundPushed, RoundError> {
        // Same critical section as `publish`/`gc`: read snapshot → write disk → swap,
        // so a round push can't race a publish/GC swap and lose an update.
        let _guard = self.lock_mutation();
        let current = self.host.snapshot();

        // The page must be served AND owned by this tenant. Both a missing page and a
        // page owned by someone else return the same opaque error (no page-existence
        // oracle across tenants).
        let sp = match current.space(slug) {
            Some(sp) if self.page_owned_by(slug, tenant) => sp,
            _ => return Err(RoundError::NoSuchPage),
        };

        let dir = self.pages_dir.join(slug);
        let round = self.read_live_round(&dir).saturating_add(1);
        let content_version = crate::submissions::content_version(&html);

        // Persist the overlay durably BEFORE swapping the served body, so a crash
        // mid-push leaves at worst a body with no `live.json` (which `load_live_body`
        // ignores → the page reverts to the immutable baseline, never blanks).
        self.write_live(&dir, &html, round)
            .map_err(RoundError::Io)?;

        // Swap the served snapshot body in place (title stays the baseline's).
        let title = sp
            .artifacts
            .get(SINGLE_SLUG)
            .map(|a| a.title.clone())
            .unwrap_or_else(|| slug.to_string());
        let mut spaces = current.spaces.clone();
        spaces.insert(slug.to_string(), one_artifact_space(html, title));
        self.host.swap(Snapshot { spaces });

        // Push the keyed round-swap to any connected shell (reuses the reload SSE).
        self.host.notify_round(slug, &content_version, round);

        Ok(RoundPushed {
            round,
            content_version,
        })
    }

    /// Durably materialize the live overlay for a page: write `live.html` (fsync +
    /// atomic rename), then `live.json` **after** it, so the meta's presence implies a
    /// matching body. Both land under the (already-durable) page directory, which is
    /// fsync'd after each rename. `.live.*.tmp` staging siblings are cleaned on error;
    /// they live under the page dir (not `pages/`), so the top-level GC tmp-reaper
    /// never touches them, and `load_page` only reads the two exact filenames.
    fn write_live(&self, dir: &Path, html: &str, round: u64) -> std::io::Result<()> {
        // Body cap (defense-in-depth; the handler already bounds it to the per-file
        // cap): never persist an overlay larger than a baseline artifact.
        if html.len() as u64 > space::MAX_FILE_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "live round body exceeds the per-file limit",
            ));
        }
        let staged = (|| -> std::io::Result<()> {
            // 1. Body first.
            let body_final = dir.join(LIVE_FILE);
            let body_tmp = dir.join(".live.html.tmp");
            write_file_synced(&body_tmp, html.as_bytes())?;
            std::fs::rename(&body_tmp, &body_final)?;
            fsync_dir(dir)?;
            // 2. Meta after the body is durable, stamped with the body's digest so a
            //    crash between the two renames yields a mismatched pair that load
            //    detects and rejects (reverting to baseline) instead of serving a body
            //    under the wrong round label.
            let lm = LiveMeta {
                schema: LIVE_SCHEMA,
                round,
                content_version: crate::submissions::content_version(html),
                updated_at: Utc::now(),
            };
            let json = serde_json::to_vec(&lm)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            let meta_final = dir.join(LIVE_META_FILE);
            let meta_tmp = dir.join(".live.json.tmp");
            write_file_synced(&meta_tmp, &json)?;
            std::fs::rename(&meta_tmp, &meta_final)?;
            fsync_dir(dir)?;
            Ok(())
        })();
        if staged.is_err() {
            let _ = std::fs::remove_file(dir.join(".live.html.tmp"));
            let _ = std::fs::remove_file(dir.join(".live.json.tmp"));
        }
        staged
    }

    /// Resolve an idempotency key to an already-published page for `tenant`, or
    /// `None` if there is no live, owned mapping. Reads the per-tenant mapping
    /// sidecar; a missing file (no key yet), a corrupt/invalid record, a **dangling**
    /// slug (mapped page no longer in the served snapshot — GC'd/deleted), or a page
    /// **not owned by `tenant`** all return `None` so the caller falls through to a
    /// fresh create. Isolation is layered: (1) only `<idem>/<tenant>/…` is consulted;
    /// (2) the record carries its owning tenant and must match; (3) the mapped page's
    /// own `meta.json` must record the same tenant. So a misplaced/hand-edited mapping
    /// can never hand one tenant another tenant's page.
    fn lookup_idempotent(
        &self,
        tenant: &str,
        key: &str,
        current: &Snapshot,
    ) -> Result<Option<Published>, PublishError> {
        let path = self.idem_path(tenant, key);
        let bytes = match read_capped(&path, MAX_META_BYTES) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(PublishError::Io(e)),
        };
        if bytes.len() as u64 > MAX_META_BYTES {
            // An oversize mapping file is corrupt/hostile — ignore it and create fresh.
            eprintln!(
                "glasspad host: idempotency mapping {} exceeds {MAX_META_BYTES} bytes; ignoring",
                path.display()
            );
            return Ok(None);
        }
        let rec: IdemRecord = match serde_json::from_slice(&bytes) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "glasspad host: bad idempotency mapping {}: {e}; ignoring",
                    path.display()
                );
                return Ok(None);
            }
        };
        // Reject a record with the wrong schema, an invalid slug, or one whose
        // recorded owner is not the requesting tenant (a misplaced/restored mapping).
        if rec.schema != IDEM_SCHEMA || !valid_space(&rec.slug) || rec.tenant != tenant {
            eprintln!(
                "glasspad host: idempotency mapping {} has invalid schema/slug/tenant; ignoring",
                path.display()
            );
            return Ok(None);
        }
        // Dangling check: only return the page if it is still served. A GC'd/deleted
        // page is absent from the snapshot, so the key falls through to a fresh create.
        let sp = match current.space(&rec.slug) {
            Some(sp) => sp,
            None => return Ok(None),
        };
        // Authoritative ownership check: the mapped page's own meta must record this
        // tenant. This closes the isolation gap even if a mapping was hand-edited to
        // name a slug owned by another tenant.
        if !self.page_owned_by(&rec.slug, tenant) {
            eprintln!(
                "glasspad host: idempotency mapping {} points at a page not owned by {tenant}; ignoring",
                path.display()
            );
            return Ok(None);
        }
        let title = sp
            .artifacts
            .get(SINGLE_SLUG)
            .map(|a| a.title.clone())
            .unwrap_or_else(|| rec.slug.clone());
        Ok(Some(Published {
            slug: rec.slug,
            title,
            created: false,
        }))
    }

    /// True iff the on-disk `meta.json` for `slug` records `tenant` as its owner. A
    /// missing/unreadable/corrupt meta returns `false` (fail-closed: an unverifiable
    /// page is not treated as owned).
    fn page_owned_by(&self, slug: &str, tenant: &str) -> bool {
        let meta_path = self.pages_dir.join(slug).join(META_FILE);
        let bytes = match read_capped(&meta_path, MAX_META_BYTES) {
            Ok(b) if b.len() as u64 <= MAX_META_BYTES => b,
            _ => return false,
        };
        serde_json::from_slice::<PageMeta>(&bytes)
            .map(|m| m.tenant == tenant && m.slug == slug)
            .unwrap_or(false)
    }

    /// Filesystem path of the mapping sidecar for `(tenant, key)`. The key is hashed
    /// (SHA-256, hex) so an arbitrary client-supplied string becomes a fixed-length,
    /// path-safe, collision-resistant filename — a raw key could contain `/`, `..`,
    /// or (on a case-insensitive filesystem) collide with a differently-cased key.
    /// `tenant` is a validated space name (`valid_space`), safe as a path component.
    fn idem_path(&self, tenant: &str, key: &str) -> PathBuf {
        self.idem_dir
            .join(tenant)
            .join(format!("{}.json", sha256_hex(key.as_bytes())))
    }

    /// Durably write the `key → slug` mapping for `tenant` (fsync + atomic rename).
    /// Called only **after** the page is durable, so the mapping never outlives its
    /// page across a crash. The tmp file is fsync'd, renamed into place, and the
    /// containing tenant directory fsync'd so the rename survives a crash; when the
    /// tenant directory is created for the first time, `idem_dir` is fsync'd too so
    /// the new directory entry is itself durable (without this, the very first key
    /// for a tenant could vanish on a crash despite `write_idem` returning success).
    fn write_idem(&self, tenant: &str, key: &str, slug: &str) -> std::io::Result<()> {
        let base = self.idem_dir.clone();
        self.write_mapping(&base, tenant, key, slug)
    }

    /// Durably write a `key → slug` mapping under `base_dir/<tenant>/<hash>.json`
    /// (fsync + atomic rename). Shared by the single-page idempotency tree
    /// (`idem/`) and the stable space-key tree (`space-idem/`) so both get the same
    /// crash-safety (tmp fsync → rename → tenant-dir fsync → base-dir fsync on first
    /// key for a tenant). The record carries its owning tenant so a misplaced file
    /// is rejected on read.
    fn write_mapping(
        &self,
        base_dir: &Path,
        tenant: &str,
        key: &str,
        slug: &str,
    ) -> std::io::Result<()> {
        let tenant_dir = base_dir.join(tenant);
        let tenant_dir_is_new = !tenant_dir.exists();
        std::fs::create_dir_all(&tenant_dir)?;
        let hash = sha256_hex(key.as_bytes());
        let final_path = tenant_dir.join(format!("{hash}.json"));
        let tmp_path = tenant_dir.join(format!(".{hash}.tmp"));
        let rec = IdemRecord {
            schema: IDEM_SCHEMA,
            tenant: tenant.to_string(),
            slug: slug.to_string(),
        };
        let json = serde_json::to_vec(&rec)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let staged = (|| -> std::io::Result<()> {
            write_file_synced(&tmp_path, &json)?;
            std::fs::rename(&tmp_path, &final_path)?;
            fsync_dir(&tenant_dir)?;
            if tenant_dir_is_new {
                fsync_dir(base_dir)?;
            }
            Ok(())
        })();
        if staged.is_err() {
            let _ = std::fs::remove_file(&tmp_path);
        }
        staged
    }

    /// A slug present in neither the current snapshot nor on disk. Bounded retries
    /// (a 128-bit collision is effectively impossible; the loop is belt-and-braces).
    fn fresh_slug(&self, current: &Snapshot) -> Result<String, PublishError> {
        for _ in 0..8 {
            let s = slug::generate();
            if !current.spaces.contains_key(&s)
                && !self.pages_dir.join(&s).exists()
                && !self.spaces_dir.join(&s).exists()
            {
                return Ok(s);
            }
        }
        Err(PublishError::SlugExhausted)
    }

    /// Atomically + **durably** materialize a page directory: write both files
    /// (fsync'd) into a `.<slug>.tmp` staging dir, fsync the staging dir, `rename`
    /// it into place, then fsync `pages_dir` so the rename survives a crash. A reader
    /// never sees a page dir that exists but lacks its artifact/meta, and once this
    /// returns the page is durable — which is the precondition for recording an
    /// idempotency mapping that points at it.
    fn write_page(&self, slug: &str, html: &str, meta: &PageMeta) -> std::io::Result<()> {
        let final_dir = self.pages_dir.join(slug);
        let tmp_dir = self.pages_dir.join(format!(".{slug}.tmp"));
        // Clean any stale staging dir from a prior crash.
        let _ = std::fs::remove_dir_all(&tmp_dir);
        // Do the staged writes in a closure so a failure at ANY step cleans up the
        // staging dir rather than leaking it (a distinct slug is minted next time,
        // so a leaked `.<slug>.tmp` would otherwise never be reclaimed).
        let staged = (|| -> std::io::Result<()> {
            std::fs::create_dir_all(&tmp_dir)?;
            write_file_synced(&tmp_dir.join(ARTIFACT_FILE), html.as_bytes())?;
            let meta_json = serde_json::to_vec_pretty(meta)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            write_file_synced(&tmp_dir.join(META_FILE), &meta_json)?;
            // Flush the staging dir so both file entries are durable before the
            // rename that publishes them.
            fsync_dir(&tmp_dir)?;
            // rename is atomic on the same filesystem; the staging dir is a sibling
            // so it shares the pages_dir filesystem.
            std::fs::rename(&tmp_dir, &final_dir)?;
            // Flush the parent so the rename (the page's appearance) is itself durable.
            fsync_dir(&self.pages_dir)
        })();
        if staged.is_err() {
            let _ = std::fs::remove_dir_all(&tmp_dir);
        }
        staged
    }

    /// Remove pages whose `created_at` is older than `retention` from disk, then
    /// rebuild + swap the snapshot so expired pages are no longer served. Returns
    /// the number removed. The snapshot swap happens **after** disk removal, and the
    /// new snapshot is rebuilt by re-scanning disk, so the served set exactly
    /// matches what survives on disk (no window serving a deleted page). Also reclaims
    /// idempotency mappings whose page is gone and leftover staging entries from a
    /// crashed write, bounding both trees to what is actually live.
    pub fn gc(&self, retention: Duration) -> std::io::Result<usize> {
        // Same lock as `publish`: GC's scan+swap must not race a publish's swap
        // (which would let GC's rebuilt snapshot clobber a just-published page, or
        // a stale publish resurrect a page GC just deleted). Holding it also means no
        // write is mid-flight, so any `.<…>.tmp` staging entry is a crash remnant we
        // can safely reap.
        let _guard = self.lock_mutation();
        let cutoff = Utc::now() - retention;
        let mut removed = 0usize;
        let rd = std::fs::read_dir(&self.pages_dir)?;
        for entry in rd.flatten() {
            let dir = entry.path();
            let name = entry.file_name();
            // Reap a leftover `.<slug>.tmp` staging dir from a crashed `write_page`
            // (a distinct slug is minted each time, so it would otherwise never be
            // reclaimed).
            if name.to_string_lossy().starts_with('.') {
                let _ = std::fs::remove_dir_all(&dir);
                continue;
            }
            if !dir.is_dir() {
                continue;
            }
            // A page is expired if its meta says so. An unreadable meta is left in
            // place (scan_disk already skips it from serving); GC only removes pages
            // it can positively date as expired.
            let created = match self.read_created_at(&dir) {
                Some(t) => t,
                None => continue,
            };
            if created < cutoff {
                if let Err(e) = std::fs::remove_dir_all(&dir) {
                    eprintln!("glasspad host: GC failed to remove {}: {e}", dir.display());
                    continue;
                }
                removed += 1;
            }
        }
        // Same pass over the multi-artifact `spaces/` tree (Gap 1). First reconcile
        // crash-staging dirs — a `.<slug>.old` whose live space is missing is RESTORED
        // (never reaped), so an interrupted in-place update is not lost; other dotdirs
        // are incomplete-write remnants. Then remove any space positively dated as
        // expired. Retention for a space is measured from its **last update**
        // (`updated_at`), so an actively-maintained docsite keeps its lease and an
        // abandoned one still expires a window after the final publish.
        if let Err(e) = self.recover_space_staging() {
            eprintln!("glasspad host: GC space staging recovery failed: {e}");
        }
        for entry in std::fs::read_dir(&self.spaces_dir)?.flatten() {
            let dir = entry.path();
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                // Recovery above already reconciled these; anything left is reclaimable.
                let _ = std::fs::remove_dir_all(&dir);
                continue;
            }
            if !dir.is_dir() {
                continue;
            }
            let last_update = match self.read_space_updated_at(&dir) {
                Some(t) => t,
                None => continue,
            };
            if last_update < cutoff {
                if let Err(e) = std::fs::remove_dir_all(&dir) {
                    eprintln!(
                        "glasspad host: GC failed to remove space {}: {e}",
                        dir.display()
                    );
                    continue;
                }
                removed += 1;
            }
        }
        if removed > 0 {
            // Make the removals durable, then rebuild + swap the served snapshot from
            // what remains on disk.
            fsync_dir(&self.pages_dir)?;
            fsync_dir(&self.spaces_dir)?;
            let snap = self.scan_disk();
            self.sweep_idem_mappings(&snap);
            self.host.swap(snap);
        } else {
            // No page expired, but still reclaim any pre-existing dangling mappings /
            // leftover mapping tmp files (e.g. from a prior crash) against the live set.
            let snap = self.host.snapshot();
            self.sweep_idem_mappings(&snap);
        }
        Ok(removed)
    }

    /// Delete idempotency mappings that no longer point at a served page, plus any
    /// leftover `.<hash>.tmp` staging files, across every tenant. A mapping is dead
    /// once its page is GC'd (a repeat with that key already falls through to a fresh
    /// create); reclaiming it here bounds the idem tree to live pages + one GC pass.
    /// Best-effort: individual failures are logged, not fatal.
    fn sweep_idem_mappings(&self, live: &Snapshot) {
        let idem = self.idem_dir.clone();
        self.sweep_mappings(&idem, live);
        let space_idem = self.space_idem_dir.clone();
        self.sweep_mappings(&space_idem, live);
    }

    /// Delete mappings under `base_dir` that no longer point at a served slug, plus
    /// any leftover `.<hash>.tmp` staging files, across every tenant. Shared by the
    /// page-idempotency and stable-space-key trees (a mapping is dead once its target
    /// slug leaves the served snapshot).
    fn sweep_mappings(&self, base_dir: &Path, live: &Snapshot) {
        let tenants = match std::fs::read_dir(base_dir) {
            Ok(rd) => rd,
            Err(e) => {
                eprintln!(
                    "glasspad host: GC cannot read mapping dir {}: {e}",
                    base_dir.display()
                );
                return;
            }
        };
        for tenant_entry in tenants.flatten() {
            let tenant_dir = tenant_entry.path();
            if !tenant_dir.is_dir() {
                continue;
            }
            let files = match std::fs::read_dir(&tenant_dir) {
                Ok(rd) => rd,
                Err(_) => continue,
            };
            for file_entry in files.flatten() {
                let path = file_entry.path();
                let name = file_entry.file_name();
                let name = name.to_string_lossy();
                // Reap leftover staging files from a crashed `write_idem`.
                if name.starts_with('.') {
                    let _ = std::fs::remove_file(&path);
                    continue;
                }
                // Delete a mapping whose target slug is no longer served, or which is
                // unreadable/corrupt (either way it can no longer resolve to a page).
                let dead = match read_capped(&path, MAX_META_BYTES) {
                    Ok(b) if b.len() as u64 <= MAX_META_BYTES => {
                        match serde_json::from_slice::<IdemRecord>(&b) {
                            Ok(rec) => !live.spaces.contains_key(&rec.slug),
                            Err(_) => true,
                        }
                    }
                    _ => true,
                };
                if dead {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }

    fn read_created_at(&self, dir: &Path) -> Option<DateTime<Utc>> {
        let bytes = read_capped(&dir.join(META_FILE), MAX_META_BYTES).ok()?;
        if bytes.len() as u64 > MAX_META_BYTES {
            return None;
        }
        let meta: PageMeta = serde_json::from_slice(&bytes).ok()?;
        Some(meta.created_at)
    }

    // --- Gap 1: multi-artifact hosted spaces -------------------------------

    /// Publish a multi-artifact **space** for `tenant`, or — with a stable
    /// `space_key` naming a space this tenant already published — **update that space
    /// in place** at the same slug/URL. `space` is already validated by
    /// [`space::build_space_bundle`]; this method owns durability + snapshot swap.
    ///
    /// Update-in-place is **owner-scoped**: the stable-key mapping is read only from
    /// this tenant's own `space-idem/<tenant>/` subtree, the mapping records its
    /// owning tenant, and the target space's own `meta.json` must record the same
    /// tenant — so a tenant can only ever replace *its own* space. A key that is
    /// absent, dangling (space GC'd), or not-owned falls through to a fresh mint (and
    /// re-points the mapping). No key → a fresh slug every publish.
    pub fn publish_space(
        &self,
        tenant: &str,
        space: Space,
        space_key: Option<&str>,
    ) -> Result<PublishedSpace, PublishError> {
        // Same critical section as page publish / round push / GC: read snapshot →
        // write disk → swap, serialized so no concurrent mutation loses an update.
        let _guard = self.lock_mutation();
        let current = self.host.snapshot();

        // Resolve an in-place target: a served space, owned by this tenant, mapped
        // by the stable key. Anything else → mint fresh.
        let existing = match space_key {
            Some(key) => self.lookup_space_slug(tenant, key, &current)?,
            None => None,
        };

        let (slug, created) = match existing {
            Some(slug) => (slug, false),
            None => {
                if current.spaces.len() >= MAX_PAGES {
                    return Err(PublishError::Full);
                }
                (self.fresh_slug(&current)?, true)
            }
        };

        let now = Utc::now();
        // A create stamps `created_at` now; an in-place update preserves the original
        // so the retention window is measured from first publish, not last refresh.
        let created_at = if created {
            now
        } else {
            self.read_space_created_at(&self.spaces_dir.join(&slug))
                .unwrap_or(now)
        };
        let meta = SpaceMeta {
            schema: SPACE_META_SCHEMA,
            slug: slug.clone(),
            tenant: tenant.to_string(),
            title: space.title.clone(),
            nav: space.nav.clone(),
            nav_groups: space.nav_groups.clone(),
            home: space.home.clone(),
            favicon: space.favicon.clone(),
            created_at,
            updated_at: now,
        };

        self.materialize_space(&slug, &space, &meta, !created)
            .map_err(PublishError::Io)?;

        // Record the durable stable-key mapping only AFTER the space is on disk (same
        // ordering as the page idempotency mapping): a crash between the two leaves at
        // worst an orphan space with no mapping, never a mapping to a missing space.
        if let Some(key) = space_key {
            self.write_space_idem(tenant, key, &slug)
                .map_err(PublishError::Io)?;
        }

        let pages: Vec<PublishedPage> = space
            .nav
            .iter()
            .filter_map(|s| {
                space.artifacts.get(s).map(|a| PublishedPage {
                    slug: s.clone(),
                    title: a.title.clone(),
                })
            })
            .collect();

        // Clone-modify-swap the served snapshot (readers in flight keep the old Arc).
        let mut spaces = current.spaces.clone();
        spaces.insert(slug.clone(), space.clone());
        self.host.swap(Snapshot { spaces });

        Ok(PublishedSpace {
            slug,
            title: space.title,
            home: space.home,
            pages,
            created,
        })
    }

    /// Replace an **existing** space's content in place, addressed by its capability
    /// `slug` (the `/p/<slug>/` the caller already holds) rather than by a stable
    /// key. Owner-scoped and **fail-if-missing**: unlike [`Store::publish_space`]'s
    /// `space_key` (which falls through to a fresh mint when the key is
    /// absent/dangling/foreign), naming a slug that is not a live space **owned by
    /// this tenant** is an opaque [`UpdateError::NoSuchSpace`] — you are targeting a
    /// specific existing resource, so a miss is an error, never a create. This gives
    /// the "I published, got a link, now update THAT link" flow a durable path
    /// without forethought (no `space_key` had to be set at first publish).
    ///
    /// Reuses the exact atomic staged-replace ([`Store::materialize_space`] with
    /// `replace = true`) + snapshot-swap the keyed update path uses, so an existing
    /// page never blanks and a crash leaves either the old or the new tree intact.
    /// The retention clock is preserved: `created_at` stays the first publish's,
    /// `updated_at` advances (so an actively-updated doc keeps its lease exactly like
    /// a keyed re-publish). No cross-tenant existence oracle: a missing slug and a
    /// foreign-owned slug return the identical error.
    pub fn update_space(
        &self,
        tenant: &str,
        slug: &str,
        space: Space,
    ) -> Result<PublishedSpace, UpdateError> {
        // Same critical section as publish / round push / GC.
        let _guard = self.lock_mutation();
        let current = self.host.snapshot();

        // Authoritative owner-scope under the lock (TOCTOU-safe): the target must be a
        // currently-served space whose own on-disk meta records THIS tenant. A slug
        // that is absent, a single-artifact page, or owned by another tenant all fail
        // closed with the one opaque error — no cross-tenant existence oracle.
        if !current.spaces.contains_key(slug) {
            return Err(UpdateError::NoSuchSpace);
        }
        // Fail closed on a page/space slug collision (a corrupt/tampered store —
        // `fresh_slug` keeps the two trees disjoint at mint). The served snapshot loads
        // pages first, so under a collision the served body is the page while a
        // hand-planted `spaces/<slug>/meta.json` could still name the caller; replacing
        // `spaces/<slug>` here would leave the page shadowing it on the next rescan.
        // Refuse to write into an ambiguous slug — parity with `page_tenant`'s
        // fail-closed-on-collision stance.
        if self.pages_dir.join(slug).exists() {
            return Err(UpdateError::NoSuchSpace);
        }

        // ONE validated meta read under the lock — the authoritative ownership check AND
        // the source of the preserved `created_at`. Full validation (schema, own slug,
        // owning tenant, name grammar) matches the loaders' defense-in-depth: a foreign,
        // wrong-schema, or hand-tampered meta fails closed (`NoSuchSpace`), and an
        // unreadable meta for a supposedly-live space is never silently turned into a
        // fresh-timestamp update. (Replaces the previous `space_owned_by` +
        // `read_space_created_at` double read of the same file.)
        let existing = match self.read_space_meta(&self.spaces_dir.join(slug)) {
            Some(m)
                if m.schema == SPACE_META_SCHEMA
                    && m.slug == slug
                    && m.tenant == tenant
                    && valid_space(&m.slug)
                    && valid_space(&m.tenant) =>
            {
                m
            }
            _ => return Err(UpdateError::NoSuchSpace),
        };

        let now = Utc::now();
        // `created_at` is carried over from the first publish (provenance). It does NOT
        // by itself preserve the retention lease: GC dates a space by `updated_at` (an
        // activity lease), so a successful update — like a keyed re-publish — advances
        // `updated_at` and thereby extends the lease. Intended and matches `publish_space`.
        let meta = SpaceMeta {
            schema: SPACE_META_SCHEMA,
            slug: slug.to_string(),
            tenant: tenant.to_string(),
            title: space.title.clone(),
            nav: space.nav.clone(),
            nav_groups: space.nav_groups.clone(),
            home: space.home.clone(),
            favicon: space.favicon.clone(),
            created_at: existing.created_at,
            updated_at: now,
        };

        // Atomic staged replace (backup the current tree, swing the new one in).
        self.materialize_space(slug, &space, &meta, true)
            .map_err(UpdateError::Io)?;

        let pages: Vec<PublishedPage> = space
            .nav
            .iter()
            .filter_map(|s| {
                space.artifacts.get(s).map(|a| PublishedPage {
                    slug: s.clone(),
                    title: a.title.clone(),
                })
            })
            .collect();

        // Clone-modify-swap the served snapshot (readers in flight keep the old Arc).
        let mut spaces = current.spaces.clone();
        spaces.insert(slug.to_string(), space.clone());
        self.host.swap(Snapshot { spaces });

        Ok(PublishedSpace {
            slug: slug.to_string(),
            title: space.title,
            home: space.home,
            pages,
            created: false,
        })
    }

    /// Scan the `spaces/` tree into `snap`. Each subdirectory is one multi-artifact
    /// space; unreadable/corrupt/invalid spaces are skipped + logged (one bad space
    /// never stops the server serving the rest).
    fn scan_spaces_into(&self, snap: &mut Snapshot) {
        let rd = match std::fs::read_dir(&self.spaces_dir) {
            Ok(rd) => rd,
            Err(e) => {
                eprintln!(
                    "glasspad host: cannot read spaces dir {}: {e}",
                    self.spaces_dir.display()
                );
                return;
            }
        };
        for entry in rd.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            match self.load_space(&dir) {
                Ok(Some((meta, space))) => {
                    // Defense-in-depth: `fresh_slug` prevents a page/space slug
                    // collision at mint, but a tampered/corrupt store could still
                    // present one. Never let a space silently overwrite a page already
                    // loaded under the same slug — skip + log rather than pick a winner
                    // (which would type-confuse the page-scoped mutation paths).
                    if snap.spaces.contains_key(&meta.slug) {
                        eprintln!(
                            "glasspad host: space slug {} collides with an already-loaded unit; skipping",
                            meta.slug
                        );
                        continue;
                    }
                    snap.spaces.insert(meta.slug.clone(), space);
                }
                Ok(None) => {}
                Err(e) => eprintln!(
                    "glasspad host: skipping unreadable space {}: {e}",
                    dir.display()
                ),
            }
        }
    }

    /// Load one `spaces/<slug>/` directory back into `(meta, Space)`, re-validating
    /// every field. The raw artifact/asset bytes are re-run through
    /// [`space::build_space_bundle`] — the **same** validation the ingest path uses —
    /// so a hand-tampered store can never smuggle a bad slug/oversize/traversal entry
    /// into the router. Symlinks (dir or any file) are rejected. Returns `Ok(None)`
    /// for a dir that is absent-meta / mismatched / fails validation (skipped, not
    /// fatal).
    fn load_space(&self, dir: &Path) -> std::io::Result<Option<(SpaceMeta, Space)>> {
        if std::fs::symlink_metadata(dir)?.file_type().is_symlink() {
            eprintln!(
                "glasspad host: space dir {} is a symlink; skipping",
                dir.display()
            );
            return Ok(None);
        }
        let meta_path = dir.join(META_FILE);
        if !meta_path.is_file() {
            return Ok(None);
        }
        let meta_bytes = read_capped(&meta_path, MAX_META_BYTES)?;
        if meta_bytes.len() as u64 > MAX_META_BYTES {
            eprintln!(
                "glasspad host: space meta.json in {} exceeds {MAX_META_BYTES} bytes; skipping",
                dir.display()
            );
            return Ok(None);
        }
        let meta: SpaceMeta = match serde_json::from_slice(&meta_bytes) {
            Ok(m) => m,
            Err(e) => {
                eprintln!(
                    "glasspad host: bad space meta.json in {}: {e}",
                    dir.display()
                );
                return Ok(None);
            }
        };
        let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let title_ok = meta
            .title
            .as_ref()
            .is_none_or(|t| t.chars().count() <= space::MAX_TITLE_CHARS);
        if meta.schema != SPACE_META_SCHEMA
            || !valid_space(&meta.slug)
            || meta.slug != dir_name
            || !valid_space(&meta.tenant)
            || !title_ok
        {
            eprintln!(
                "glasspad host: space dir {} has invalid meta (schema/slug/tenant/title); skipping",
                dir.display()
            );
            return Ok(None);
        }

        // Collect the raw pages + assets off disk under a shared per-space budget
        // (bounded reads; symlinks — including the subdir roots — rejected), so a
        // hand-tampered store can never force an unbounded allocation on startup / GC.
        let mut budget = LoadBudget::new();
        let pages = match self.read_space_pages(dir, &mut budget)? {
            Some(p) => p,
            None => return Ok(None),
        };
        let assets = match self.read_space_assets(dir, &mut budget)? {
            Some(a) => a,
            None => return Ok(None),
        };

        // Re-validate through the SAME builder the ingest surface uses. The favicon
        // is producer/repo metadata (not derived from the artifact files), so it is
        // reattached from the meta after the builder — re-validated defensively so a
        // hand-tampered `meta.json` can never smuggle a non-emoji favicon into a shell.
        match build_space_bundle(
            pages,
            assets,
            meta.nav.clone(),
            meta.nav_groups.clone(),
            meta.title.clone(),
        ) {
            Ok(mut sp) => {
                // The favicon is decorative, so a corrupt/tampered value must not skip
                // the whole space — but it is logged (not silently dropped) and the page
                // reverts to the default, matching the diagnostics the rest of load emits.
                sp.favicon = match meta.favicon.as_deref() {
                    None => None,
                    Some(raw) => match crate::favicon::validate(raw) {
                        Ok(v) => Some(v),
                        Err(e) => {
                            eprintln!(
                                "glasspad host: space {} has an invalid stored favicon: {e}; \
                                 using the default",
                                meta.slug
                            );
                            None
                        }
                    },
                };
                Ok(Some((meta, sp)))
            }
            Err(e) => {
                eprintln!(
                    "glasspad host: space {} failed revalidation: {e}; skipping",
                    dir.display()
                );
                Ok(None)
            }
        }
    }

    /// Read a space's `artifacts/<page>.html` bodies into [`BundlePage`]s. Returns
    /// `Ok(None)` (skip the whole space, fail-closed) if a symlink is encountered — the
    /// `artifacts/` root itself OR any entry — or the running budget is exceeded.
    /// `budget` accumulates the per-space byte total and entry count across pages AND
    /// assets so a hand-tampered store can never force an unbounded allocation on load.
    fn read_space_pages(
        &self,
        dir: &Path,
        budget: &mut LoadBudget,
    ) -> std::io::Result<Option<Vec<BundlePage>>> {
        let art_dir = dir.join(SPACE_ARTIFACTS_DIR);
        // Reject a symlinked `artifacts/` root before reading (read_dir would follow it
        // outside the store). Absent → an empty page set (build rejects an empty space).
        match std::fs::symlink_metadata(&art_dir) {
            Ok(md) if md.file_type().is_symlink() => {
                eprintln!(
                    "glasspad host: space artifacts dir {} is a symlink; skipping space",
                    art_dir.display()
                );
                return Ok(None);
            }
            Ok(md) if !md.is_dir() => return Ok(Some(Vec::new())),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Some(Vec::new())),
            Err(e) => return Err(e),
        }
        let mut pages = Vec::new();
        for entry in std::fs::read_dir(&art_dir)?.flatten() {
            let path = entry.path();
            let ft = std::fs::symlink_metadata(&path)?.file_type();
            if ft.is_symlink() {
                eprintln!(
                    "glasspad host: space artifact {} is a symlink; skipping space",
                    path.display()
                );
                return Ok(None);
            }
            if !ft.is_file() {
                continue;
            }
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            let slug = match name.strip_suffix(".html") {
                Some(s) => s.to_string(),
                None => continue,
            };
            if !budget.reserve_entry() {
                return Ok(None);
            }
            let html = read_capped_utf8(&path)?;
            if !budget.add_bytes(html.len() as u64) {
                return Ok(None);
            }
            pages.push(BundlePage { slug, html });
        }
        Ok(Some(pages))
    }

    /// Read a space's `assets/` subtree into [`BundleAsset`]s keyed by the path
    /// **relative to** `assets/` (no prefix — the shape `build_space_bundle` expects).
    /// Returns `Ok(None)` (skip the whole space, fail-closed) on a symlinked `assets/`
    /// root or entry, a non-`Component::Normal`/non-UTF-8 path component, or a budget
    /// overrun. `budget` is shared with [`Store::read_space_pages`].
    fn read_space_assets(
        &self,
        dir: &Path,
        budget: &mut LoadBudget,
    ) -> std::io::Result<Option<Vec<BundleAsset>>> {
        let assets_root = dir.join(SPACE_ASSETS_SUBDIR);
        // Reject a symlinked `assets/` root before reading (read_dir would follow it).
        match std::fs::symlink_metadata(&assets_root) {
            Ok(md) if md.file_type().is_symlink() => {
                eprintln!(
                    "glasspad host: space assets dir {} is a symlink; skipping space",
                    assets_root.display()
                );
                return Ok(None);
            }
            Ok(md) if !md.is_dir() => return Ok(Some(Vec::new())),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Some(Vec::new())),
            Err(e) => return Err(e),
        }
        let mut out = Vec::new();
        let mut stack = vec![assets_root.clone()];
        while let Some(cur) = stack.pop() {
            for entry in std::fs::read_dir(&cur)?.flatten() {
                let path = entry.path();
                let ft = std::fs::symlink_metadata(&path)?.file_type();
                if ft.is_symlink() {
                    eprintln!(
                        "glasspad host: space asset {} is a symlink; skipping space",
                        path.display()
                    );
                    return Ok(None);
                }
                if ft.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !ft.is_file() {
                    continue;
                }
                let rel = match path.strip_prefix(&assets_root) {
                    Ok(r) => r,
                    Err(_) => return Ok(None),
                };
                // Only `Normal` path components map to an asset key; anything else
                // (`.`/`..`/root/prefix) or a non-UTF-8 name fails the whole space
                // closed (matches the scanner's `rel_key`).
                let mut segs = Vec::new();
                for comp in rel.components() {
                    match comp {
                        std::path::Component::Normal(os) => match os.to_str() {
                            Some(s) => segs.push(s.to_string()),
                            None => return Ok(None),
                        },
                        _ => return Ok(None),
                    }
                }
                if !budget.reserve_entry() {
                    return Ok(None);
                }
                let bytes = read_capped(&path, space::MAX_FILE_BYTES)?;
                if !budget.add_bytes(bytes.len() as u64) {
                    return Ok(None);
                }
                out.push(BundleAsset {
                    path: segs.join("/"),
                    bytes,
                });
            }
        }
        Ok(Some(out))
    }

    /// Atomically + durably materialize a `spaces/<slug>/` directory from a validated
    /// [`Space`] + [`SpaceMeta`]. Stages the whole tree in `.<slug>.tmp/`, fsyncs it,
    /// then publishes it with a single rename (fresh create) or an atomic replace
    /// (rename final → `.<slug>.old`, rename tmp → final, remove backup) so a reader
    /// never sees a half-written or missing space and a crash leaves either the old
    /// or the new tree intact.
    ///
    /// **Commit invariant (what `Ok`/`Err` mean to the caller).** The publishing
    /// rename (`tmp → final`) is the single **commit point**. `Err` is returned only
    /// for a failure *before* that rename lands — in which case nothing was swapped
    /// into `spaces/<slug>/` (a replace restored its prior tree, a create left no
    /// final), so the caller's on-disk truth is unchanged and it must **not** swap its
    /// served snapshot. Once the rename succeeds the new tree *is* the on-disk truth,
    /// so this function **commits to `Ok`**: the post-commit parent-dir fsync is
    /// best-effort (see [`Store::commit_fsync`]) and its failure is logged, never
    /// surfaced. If it were surfaced, the caller would get `Err`, skip the snapshot
    /// swap believing "nothing changed", yet a restart — which rebuilds the snapshot
    /// from disk — would serve the new tree, diverging memory from disk until then.
    /// So callers can rely on `Ok ⟺ new tree is final` and `Err ⟺ nothing changed`,
    /// and swap iff `Ok`.
    fn materialize_space(
        &self,
        slug: &str,
        space: &Space,
        meta: &SpaceMeta,
        replace: bool,
    ) -> std::io::Result<()> {
        let final_dir = self.spaces_dir.join(slug);
        let tmp_dir = self.spaces_dir.join(format!(".{slug}.tmp"));
        let backup_dir = self.spaces_dir.join(format!(".{slug}.old"));
        let _ = std::fs::remove_dir_all(&tmp_dir);

        let staged = (|| -> std::io::Result<()> {
            std::fs::create_dir_all(&tmp_dir)?;
            // meta.json
            let meta_json = serde_json::to_vec_pretty(meta)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            write_file_synced(&tmp_dir.join(META_FILE), &meta_json)?;
            // artifacts/<page>.html
            let art_dir = tmp_dir.join(SPACE_ARTIFACTS_DIR);
            std::fs::create_dir_all(&art_dir)?;
            for (page_slug, artifact) in &space.artifacts {
                write_file_synced(
                    &art_dir.join(format!("{page_slug}.html")),
                    artifact.html.as_bytes(),
                )?;
            }
            // assets/<rel...> — the key already begins with `assets/`, so joining it
            // onto the space dir reproduces the `assets/<rel>` layout.
            for (key, asset) in &space.assets {
                let dest = tmp_dir.join(key);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                write_file_synced(&dest, &asset.bytes)?;
            }
            // Flush every directory in the staged tree so its entries are durable
            // before the publishing rename.
            fsync_tree(&tmp_dir)?;

            if replace {
                let _ = std::fs::remove_dir_all(&backup_dir);
                // Move the current tree aside, swing the new one in, then drop the
                // backup. A crash between the renames leaves a reclaimable `.old`.
                let moved_aside = final_dir.exists();
                if moved_aside {
                    std::fs::rename(&final_dir, &backup_dir)?;
                }
                // If swinging the new tree in fails (an ordinary, non-crash I/O error),
                // restore the backup so the live space is never left MISSING — crash
                // recovery (`recover_space_staging`) only runs at startup / hourly GC,
                // so a runtime failure has to self-heal synchronously here rather than
                // stranding the only committed generation in `.old`. This is a
                // PRE-COMMIT failure: nothing was swapped into `final_dir`, so the
                // returned `Err` truthfully means "nothing changed" and the caller
                // must not swap its snapshot.
                if let Err(e) = std::fs::rename(&tmp_dir, &final_dir) {
                    if moved_aside && !final_dir.exists() {
                        let _ = std::fs::rename(&backup_dir, &final_dir);
                        let _ = fsync_dir(&self.spaces_dir);
                    }
                    return Err(e);
                }
                // COMMIT POINT reached: the new tree is now `spaces/<slug>/`, the
                // on-disk truth. Everything past here is best-effort and must NOT turn
                // a committed replace back into an `Err` (see the commit invariant on
                // this fn and `commit_fsync`) — the caller has to swap its snapshot.
                if let Err(e) = self.commit_fsync(&self.spaces_dir) {
                    eprintln!(
                        "glasspad host: fsync of spaces dir after committing replace of {slug} \
                         failed: {e}; the new tree is live but its durability is unconfirmed \
                         until the OS flushes"
                    );
                }
                let _ = std::fs::remove_dir_all(&backup_dir);
            } else {
                std::fs::rename(&tmp_dir, &final_dir)?;
                // COMMIT POINT: the fresh tree is now `spaces/<slug>/`. As in the
                // replace branch, the parent-dir flush past this point is best-effort —
                // a failure must not make the caller believe the create did not happen
                // (it did; a restart would serve it), so we log and commit to `Ok`.
                if let Err(e) = self.commit_fsync(&self.spaces_dir) {
                    eprintln!(
                        "glasspad host: fsync of spaces dir after creating {slug} failed: {e}; \
                         the new tree is live but its durability is unconfirmed until the OS flushes"
                    );
                }
            }
            Ok(())
        })();
        if staged.is_err() {
            let _ = std::fs::remove_dir_all(&tmp_dir);
        }
        staged
    }

    /// fsync the `spaces/` parent after a **committed** publishing rename. This is the
    /// durability flush for the rename, but it runs *past the point of no return*: the
    /// new tree is already the on-disk truth, so [`Store::materialize_space`]
    /// deliberately swallows an error here rather than reverting a committed replace to
    /// `Err` (which would strand the caller believing "nothing changed" while a restart
    /// serves the new tree — the divergence this whole design prevents). It is split
    /// into its own method so a test can deterministically simulate that flush failing
    /// and prove the caller still commits + swaps.
    fn commit_fsync(&self, dir: &Path) -> std::io::Result<()> {
        #[cfg(test)]
        if fault::take_commit_fsync_fault() {
            return Err(std::io::Error::other("injected post-commit fsync failure"));
        }
        fsync_dir(dir)
    }

    /// Resolve a stable space key to a served space slug **owned by `tenant`**, or
    /// `None` if there is no live, owned mapping. Mirrors [`Store::lookup_idempotent`]'s
    /// layered isolation: only `<space-idem>/<tenant>/…` is read; the record's tenant
    /// must match; and the target space's own `meta.json` must record the same tenant.
    /// A dangling/corrupt/foreign mapping returns `None` (→ fresh mint).
    fn lookup_space_slug(
        &self,
        tenant: &str,
        key: &str,
        current: &Snapshot,
    ) -> Result<Option<String>, PublishError> {
        let path = self.space_idem_path(tenant, key);
        let bytes = match read_capped(&path, MAX_META_BYTES) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(PublishError::Io(e)),
        };
        if bytes.len() as u64 > MAX_META_BYTES {
            eprintln!(
                "glasspad host: space-idem mapping {} exceeds {MAX_META_BYTES} bytes; ignoring",
                path.display()
            );
            return Ok(None);
        }
        let rec: IdemRecord = match serde_json::from_slice(&bytes) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "glasspad host: bad space-idem mapping {}: {e}; ignoring",
                    path.display()
                );
                return Ok(None);
            }
        };
        if rec.schema != IDEM_SCHEMA || !valid_space(&rec.slug) || rec.tenant != tenant {
            eprintln!(
                "glasspad host: space-idem mapping {} has invalid schema/slug/tenant; ignoring",
                path.display()
            );
            return Ok(None);
        }
        // Dangling: only reuse the slug if the space is still served AND owned by this
        // tenant (authoritative check against the space's own meta).
        if !current.spaces.contains_key(&rec.slug) || !self.space_owned_by(&rec.slug, tenant) {
            return Ok(None);
        }
        Ok(Some(rec.slug))
    }

    /// True iff `spaces/<slug>/meta.json` records `tenant` as owner (and its own
    /// slug). A missing/unreadable/corrupt meta returns `false` (fail-closed).
    fn space_owned_by(&self, slug: &str, tenant: &str) -> bool {
        let meta_path = self.spaces_dir.join(slug).join(META_FILE);
        let bytes = match read_capped(&meta_path, MAX_META_BYTES) {
            Ok(b) if b.len() as u64 <= MAX_META_BYTES => b,
            _ => return false,
        };
        serde_json::from_slice::<SpaceMeta>(&bytes)
            .map(|m| m.tenant == tenant && m.slug == slug)
            .unwrap_or(false)
    }

    /// The `created_at` recorded in a space directory's `meta.json`, or `None`.
    fn read_space_created_at(&self, dir: &Path) -> Option<DateTime<Utc>> {
        self.read_space_meta(dir).map(|m| m.created_at)
    }

    /// The `updated_at` recorded in a space directory's `meta.json`, or `None`. This
    /// is the retention clock for a space (activity-based lease — a re-publish extends
    /// it), distinct from the immutable single-page path which expires by `created_at`.
    fn read_space_updated_at(&self, dir: &Path) -> Option<DateTime<Utc>> {
        self.read_space_meta(dir).map(|m| m.updated_at)
    }

    /// Parse a space directory's `meta.json` (bounded read), or `None`.
    fn read_space_meta(&self, dir: &Path) -> Option<SpaceMeta> {
        let bytes = read_capped(&dir.join(META_FILE), MAX_META_BYTES).ok()?;
        if bytes.len() as u64 > MAX_META_BYTES {
            return None;
        }
        serde_json::from_slice::<SpaceMeta>(&bytes).ok()
    }

    /// Filesystem path of the stable-space-key mapping sidecar for `(tenant, key)`
    /// (key SHA-256'd for a fixed-length, path-safe filename).
    fn space_idem_path(&self, tenant: &str, key: &str) -> PathBuf {
        self.space_idem_dir
            .join(tenant)
            .join(format!("{}.json", sha256_hex(key.as_bytes())))
    }

    /// Durably write the stable-space-key `key → slug` mapping for `tenant`.
    fn write_space_idem(&self, tenant: &str, key: &str, slug: &str) -> std::io::Result<()> {
        let base = self.space_idem_dir.clone();
        self.write_mapping(&base, tenant, key, slug)
    }
}

/// Build a single-artifact [`Space`] (home = `index`) from a body + resolved title.
/// Mirrors `server::one_artifact_snapshot`'s space shape.
fn one_artifact_space(html: String, title: String) -> Space {
    let mut sp = Space::default();
    sp.artifacts
        .insert(SINGLE_SLUG.to_string(), Artifact { html, title });
    sp.nav = vec![SINGLE_SLUG.to_string()];
    sp.home = Some(SINGLE_SLUG.to_string());
    sp
}

/// Hex-encode the SHA-256 of `bytes` (64 lowercase hex chars). Used to turn an
/// arbitrary idempotency key into a fixed-length, path-safe, collision-resistant
/// filename.
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        // Two lowercase hex digits per byte; infallible into a String.
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Write `bytes` to `path` and fsync the file so its contents are durable before
/// the caller renames it into place (create truncates any stale tmp file).
fn write_file_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut f = std::fs::File::create(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}

/// fsync a directory so a rename/create within it is durable. On Unix this flushes
/// the directory entry; on other platforms a directory handle can't be fsync'd the
/// same way, so it is a no-op (the deploy target is Unix — see the crate's
/// `cfg(unix)` deps).
#[cfg(unix)]
fn fsync_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::File::open(dir)?.sync_all()
}

#[cfg(not(unix))]
fn fsync_dir(_dir: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Recursively fsync `dir` and every subdirectory beneath it, so all directory
/// entries in a staged tree are durable before it is renamed into place. Files are
/// already fsync'd by `write_file_synced`; this flushes the containing directories
/// (their entries) which a file fsync does not cover.
fn fsync_tree(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            fsync_tree(&path)?;
        }
    }
    fsync_dir(dir)
}

/// Ceiling on a `meta.json` sidecar read — it is a handful of small fields, so a
/// larger file is corrupt/hostile and is bounded here to avoid an unbounded alloc.
const MAX_META_BYTES: u64 = 64 * 1024;

/// Read at most `max + 1` bytes of `path` (a bounded allocation). A returned length
/// `> max` signals over-limit without ever buffering an unbounded file.
fn read_capped(path: &Path, max: u64) -> std::io::Result<Vec<u8>> {
    let f = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    f.take(max + 1).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Bounded UTF-8 read of a stored artifact body (same per-file cap the scanner
/// and `create` enforce), so a hand-tampered oversize file can't force an
/// unbounded allocation on load.
fn read_capped_utf8(path: &Path) -> std::io::Result<String> {
    let bytes = read_capped(path, space::MAX_FILE_BYTES)?;
    if bytes.len() as u64 > space::MAX_FILE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "stored artifact exceeds the per-file limit",
        ));
    }
    String::from_utf8(bytes)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "artifact is not UTF-8"))
}

/// Outcome of a successful [`Store::push_round`].
#[derive(Debug)]
pub struct RoundPushed {
    /// The new monotonic round number (baseline is round 0; first push is round 1).
    pub round: u64,
    /// The content-version of the new round's body — the value a submission for this
    /// round must echo (cross-round binding).
    pub content_version: String,
}

/// Failures the round-push handler maps to HTTP status.
#[derive(Debug)]
pub enum RoundError {
    /// The page does not exist, or is not owned by the requesting tenant (opaque —
    /// no cross-tenant existence oracle). Maps to `404`.
    NoSuchPage,
    Io(std::io::Error),
}

impl std::fmt::Display for RoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoundError::NoSuchPage => write!(f, "no such page for this tenant"),
            RoundError::Io(e) => write!(f, "storage error: {e}"),
        }
    }
}

/// Failures the update-in-place handler (`PUT /api/v1/spaces/{slug}`) maps to HTTP
/// status.
#[derive(Debug)]
pub enum UpdateError {
    /// The addressed slug is not a live space owned by the requesting tenant —
    /// missing, a single-artifact page, or owned by someone else (opaque, no
    /// cross-tenant existence oracle). Maps to `404`.
    NoSuchSpace,
    Io(std::io::Error),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::NoSuchSpace => write!(f, "no such space for this tenant"),
            UpdateError::Io(e) => write!(f, "storage error: {e}"),
        }
    }
}

/// Publish failures the ingest handler maps to HTTP status.
#[derive(Debug)]
pub enum PublishError {
    /// The global page cap is reached.
    Full,
    /// Could not mint a fresh slug (effectively never — a broken RNG).
    SlugExhausted,
    Io(std::io::Error),
}

impl std::fmt::Display for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PublishError::Full => write!(f, "page store is at capacity ({MAX_PAGES} pages)"),
            PublishError::SlugExhausted => write!(f, "could not allocate a unique slug"),
            PublishError::Io(e) => write!(f, "storage error: {e}"),
        }
    }
}

/// Test-only deterministic fault injection for the post-commit durability flush
/// ([`Store::commit_fsync`]). Thread-local so parallel tests don't interfere; a
/// direct call chain (test → `publish_space`/`update_space` → `materialize_space`
/// → `commit_fsync`) stays on one thread, so arming here reaches the intended flush.
#[cfg(test)]
mod fault {
    use std::cell::Cell;

    thread_local! {
        /// Number of upcoming `commit_fsync` calls (this thread) to fail; decremented
        /// on each consumed fault.
        static COMMIT_FSYNC_FAULTS: Cell<u32> = const { Cell::new(0) };
    }

    /// Arm the next `n` post-commit fsyncs on the current thread to fail.
    pub fn arm_commit_fsync_faults(n: u32) {
        COMMIT_FSYNC_FAULTS.with(|c| c.set(n));
    }

    /// Consume one armed fault; `true` when the current `commit_fsync` should fail.
    pub fn take_commit_fsync_fault() -> bool {
        COMMIT_FSYNC_FAULTS.with(|c| {
            let n = c.get();
            if n > 0 {
                c.set(n - 1);
                true
            } else {
                false
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "gp-store-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn host() -> Arc<ArtifactHost> {
        Arc::new(ArtifactHost::new_public(
            "https://pad.example.com".into(),
            "/p".into(),
        ))
    }

    #[test]
    fn page_tenant_resolves_page_or_space_and_fails_closed_on_collision() {
        let root = tmp_root("page-tenant");
        let store = Store::open(&root, host()).unwrap();

        // A page-tree owner and a space-tree owner each resolve on their own.
        let page = store
            .publish("acme", "<h1>page</h1>".into(), None, None)
            .unwrap();
        assert_eq!(store.page_tenant(&page.slug).as_deref(), Some("acme"));

        let space = space::Space {
            artifacts: std::collections::BTreeMap::from([(
                "index".to_string(),
                space::Artifact {
                    html: "<h1>space</h1>".into(),
                    title: "s".into(),
                },
            )]),
            assets: Default::default(),
            nav: vec!["index".into()],
            nav_groups: vec![],
            home: Some("index".into()),
            title: None,
            favicon: None,
        };
        let sp = store.publish_space("globex", space, None).unwrap();
        assert_eq!(store.page_tenant(&sp.slug).as_deref(), Some("globex"));

        // An UNKNOWN slug is None (opaque-404 upstream).
        assert_eq!(store.page_tenant("nope-nope-nope"), None);

        // Hand-plant a page/ and space/ meta at the SAME slug owned by DIFFERENT
        // tenants — a corrupt/tampered state `fresh_slug` normally prevents. Ownership
        // is ambiguous, so `page_tenant` must FAIL CLOSED (None), never silently bind
        // a submission to whichever tree it happened to read first.
        let collide = "collideslug";
        let now = Utc::now();
        let page_dir = root.join("pages").join(collide);
        std::fs::create_dir_all(&page_dir).unwrap();
        std::fs::write(
            page_dir.join(META_FILE),
            serde_json::to_vec(&PageMeta {
                schema: META_SCHEMA,
                slug: collide.into(),
                tenant: "acme".into(),
                title: "t".into(),
                created_at: now,
            })
            .unwrap(),
        )
        .unwrap();
        let space_dir = root.join("spaces").join(collide);
        std::fs::create_dir_all(&space_dir).unwrap();
        std::fs::write(
            space_dir.join(META_FILE),
            serde_json::to_vec(&SpaceMeta {
                schema: SPACE_META_SCHEMA,
                slug: collide.into(),
                tenant: "globex".into(),
                title: None,
                nav: vec![],
                nav_groups: vec![],
                home: None,
                favicon: None,
                created_at: now,
                updated_at: now,
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            store.page_tenant(collide),
            None,
            "a slug owned by different tenants in both trees must fail closed"
        );
    }

    #[test]
    fn publish_persists_and_serves_and_survives_reopen() {
        let root = tmp_root("persist");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        assert_eq!(store.page_count(), 0);

        let pub1 = store
            .publish("acme", "<h1>Hello</h1>".into(), None, None)
            .unwrap();
        assert!(crate::artifact_host::valid_name(&pub1.slug));
        // Now served in the snapshot.
        let snap = h.snapshot();
        assert!(snap.space(&pub1.slug).is_some());
        assert_eq!(store.page_count(), 1);

        // Files exist on disk.
        assert!(
            root.join("pages")
                .join(&pub1.slug)
                .join("artifact.html")
                .is_file()
        );
        assert!(
            root.join("pages")
                .join(&pub1.slug)
                .join("meta.json")
                .is_file()
        );

        // Reopen a fresh host+store: the page reloads from disk.
        let h2 = host();
        let store2 = Store::open(&root, h2.clone()).unwrap();
        assert_eq!(store2.page_count(), 1);
        assert!(h2.snapshot().space(&pub1.slug).is_some());

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn concurrent_publishes_are_all_served() {
        // Regression for the lost-update race: without the store mutation lock, two
        // concurrent read-clone-swap publishes drop each other's page from the served
        // snapshot. With the lock, every published page is present.
        let root = tmp_root("concurrent");
        let h = host();
        let store = Arc::new(Store::open(&root, h.clone()).unwrap());
        let mut handles = Vec::new();
        for i in 0..24 {
            let store = store.clone();
            handles.push(std::thread::spawn(move || {
                store
                    .publish("acme", format!("<h1>page {i}</h1>"), None, None)
                    .unwrap()
                    .slug
            }));
        }
        let slugs: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let snap = h.snapshot();
        for s in &slugs {
            assert!(snap.space(s).is_some(), "concurrent publish lost page {s}");
        }
        assert_eq!(store.page_count(), 24);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn publish_records_authenticated_tenant_in_meta() {
        // Tenant isolation depends on the owner tag being the authenticated tenant.
        let root = tmp_root("tenantmeta");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let p = store
            .publish("globex", "<h1>x</h1>".into(), None, None)
            .unwrap();
        let meta: PageMeta = serde_json::from_slice(
            &std::fs::read(root.join("pages").join(&p.slug).join("meta.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(meta.tenant, "globex");
        assert_eq!(meta.schema, META_SCHEMA);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn each_publish_mints_a_new_slug_no_overwrite() {
        let root = tmp_root("newslug");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let a = store
            .publish("acme", "<h1>A</h1>".into(), None, None)
            .unwrap();
        let b = store
            .publish("acme", "<h1>B</h1>".into(), None, None)
            .unwrap();
        assert_ne!(a.slug, b.slug, "publish must always mint a fresh slug");
        // Both pages coexist (immutable, no overwrite).
        assert_eq!(store.page_count(), 2);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn gc_removes_expired_and_stops_serving_it() {
        let root = tmp_root("gc");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let p = store
            .publish("acme", "<h1>old</h1>".into(), None, None)
            .unwrap();

        // Backdate the page's meta.created_at to 100 days ago on disk.
        let meta_path = root.join("pages").join(&p.slug).join("meta.json");
        let mut meta: PageMeta =
            serde_json::from_slice(&std::fs::read(&meta_path).unwrap()).unwrap();
        meta.created_at = Utc::now() - Duration::days(100);
        std::fs::write(&meta_path, serde_json::to_vec_pretty(&meta).unwrap()).unwrap();

        let removed = store.gc(Duration::days(90)).unwrap();
        assert_eq!(removed, 1);
        // No longer on disk…
        assert!(!root.join("pages").join(&p.slug).exists());
        // …and no longer served.
        assert!(h.snapshot().space(&p.slug).is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn gc_keeps_fresh_pages() {
        let root = tmp_root("gckeep");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let p = store
            .publish("acme", "<h1>new</h1>".into(), None, None)
            .unwrap();
        let removed = store.gc(Duration::days(90)).unwrap();
        assert_eq!(removed, 0);
        assert!(h.snapshot().space(&p.slug).is_some());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn title_override_wins_else_resolved_from_html() {
        let root = tmp_root("title");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let a = store
            .publish(
                "acme",
                "<title>FromHtml</title><h1>x</h1>".into(),
                None,
                None,
            )
            .unwrap();
        assert_eq!(a.title, "FromHtml");
        let b = store
            .publish("acme", "<h1>x</h1>".into(), Some("Override".into()), None)
            .unwrap();
        assert_eq!(b.title, "Override");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn idempotent_repeat_returns_same_page_no_new_slug() {
        let root = tmp_root("idem-repeat");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let first = store
            .publish("acme", "<h1>one</h1>".into(), None, Some("k-1"))
            .unwrap();
        // A repeat with the same key returns the SAME slug — no new page minted.
        let again = store
            .publish("acme", "<h1>changed body</h1>".into(), None, Some("k-1"))
            .unwrap();
        assert_eq!(
            first.slug, again.slug,
            "same key must return the first page"
        );
        assert_eq!(again.title, first.title, "repeat returns the stored title");
        assert_eq!(
            store.page_count(),
            1,
            "no duplicate page for a repeated key"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn different_key_mints_a_new_page() {
        let root = tmp_root("idem-diff");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let a = store
            .publish("acme", "<h1>a</h1>".into(), None, Some("k-a"))
            .unwrap();
        let b = store
            .publish("acme", "<h1>b</h1>".into(), None, Some("k-b"))
            .unwrap();
        assert_ne!(a.slug, b.slug, "a distinct key must mint a fresh page");
        assert_eq!(store.page_count(), 2);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn same_key_is_scoped_per_tenant() {
        // Two tenants using the *same* key string must each get their own page —
        // one tenant's key can never resolve to another's page.
        let root = tmp_root("idem-tenant");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let a = store
            .publish("acme", "<h1>acme</h1>".into(), None, Some("shared"))
            .unwrap();
        let b = store
            .publish("globex", "<h1>globex</h1>".into(), None, Some("shared"))
            .unwrap();
        assert_ne!(
            a.slug, b.slug,
            "same key, different tenants → different pages"
        );
        // Each tenant's repeat still returns its own page.
        let a2 = store
            .publish("acme", "<h1>x</h1>".into(), None, Some("shared"))
            .unwrap();
        let b2 = store
            .publish("globex", "<h1>y</h1>".into(), None, Some("shared"))
            .unwrap();
        assert_eq!(a.slug, a2.slug);
        assert_eq!(b.slug, b2.slug);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn dangling_key_falls_through_to_fresh_create() {
        // A key whose page has been GC'd must not resurrect it: the next publish with
        // that key creates a fresh page (and re-points the mapping at the new slug).
        let root = tmp_root("idem-dangling");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let first = store
            .publish("acme", "<h1>old</h1>".into(), None, Some("k"))
            .unwrap();

        // Backdate + GC the page so the mapping is now dangling.
        let meta_path = root.join("pages").join(&first.slug).join("meta.json");
        let mut meta: PageMeta =
            serde_json::from_slice(&std::fs::read(&meta_path).unwrap()).unwrap();
        meta.created_at = Utc::now() - Duration::days(100);
        std::fs::write(&meta_path, serde_json::to_vec_pretty(&meta).unwrap()).unwrap();
        assert_eq!(store.gc(Duration::days(90)).unwrap(), 1);
        assert!(h.snapshot().space(&first.slug).is_none());

        // Same key again: dangling → fresh create.
        let second = store
            .publish("acme", "<h1>new</h1>".into(), None, Some("k"))
            .unwrap();
        assert_ne!(first.slug, second.slug, "dangling key must create fresh");
        // …and the refreshed mapping now returns the NEW page on a further repeat.
        let third = store
            .publish("acme", "<h1>z</h1>".into(), None, Some("k"))
            .unwrap();
        assert_eq!(second.slug, third.slug, "mapping re-points at the new page");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn idempotency_mapping_survives_reopen() {
        // The mapping is durable: after reopening the store, the same key still
        // resolves to the first page (exactly-once across a restart).
        let root = tmp_root("idem-reopen");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let first = store
            .publish("acme", "<h1>durable</h1>".into(), None, Some("k"))
            .unwrap();
        drop(store);

        let h2 = host();
        let store2 = Store::open(&root, h2.clone()).unwrap();
        let again = store2
            .publish("acme", "<h1>retry</h1>".into(), None, Some("k"))
            .unwrap();
        assert_eq!(first.slug, again.slug, "mapping must survive reopen");
        assert_eq!(store2.page_count(), 1);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn corrupt_mapping_falls_through_to_fresh_create() {
        // A hand-tampered/corrupt mapping file must not crash or return a bad page —
        // it is ignored and the publish creates fresh.
        let root = tmp_root("idem-corrupt");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let first = store
            .publish("acme", "<h1>a</h1>".into(), None, Some("k"))
            .unwrap();
        // Overwrite the mapping sidecar with garbage.
        let path = store.idem_path("acme", "k");
        std::fs::write(&path, b"not json").unwrap();
        let second = store
            .publish("acme", "<h1>b</h1>".into(), None, Some("k"))
            .unwrap();
        assert_ne!(first.slug, second.slug, "corrupt mapping → fresh create");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn no_key_mints_fresh_every_time() {
        // The default path is unchanged: no key → a new slug on every publish.
        let root = tmp_root("idem-nokey");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let a = store
            .publish("acme", "<h1>a</h1>".into(), None, None)
            .unwrap();
        let b = store
            .publish("acme", "<h1>a</h1>".into(), None, None)
            .unwrap();
        assert_ne!(a.slug, b.slug);
        assert_eq!(store.page_count(), 2);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn concurrent_same_key_publishes_produce_one_page() {
        // Many threads publishing the same (tenant, key) must yield exactly one page:
        // the mutex serializes them, the first mints + records the mapping, the rest
        // replay it. All returned slugs are identical.
        let root = tmp_root("idem-concurrent");
        let h = host();
        let store = Arc::new(Store::open(&root, h.clone()).unwrap());
        let mut handles = Vec::new();
        for i in 0..24 {
            let store = store.clone();
            handles.push(std::thread::spawn(move || {
                store
                    .publish("acme", format!("<h1>{i}</h1>"), None, Some("same-key"))
                    .unwrap()
                    .slug
            }));
        }
        let slugs: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let first = &slugs[0];
        assert!(
            slugs.iter().all(|s| s == first),
            "all same-key publishes must return one slug, got {slugs:?}"
        );
        assert_eq!(store.page_count(), 1, "exactly one page for a shared key");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn cross_tenant_mapping_is_never_honored() {
        // Isolation: even a hand-crafted mapping under tenant A that names a page
        // owned by tenant B must NOT return B's page to A. A gets a fresh page.
        let root = tmp_root("idem-crosstenant");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let victim = store
            .publish("globex", "<h1>secret</h1>".into(), None, None)
            .unwrap();

        // (a) A mapping recording the wrong owner is rejected by the record check.
        let tenant_dir = root.join("idem").join("acme");
        std::fs::create_dir_all(&tenant_dir).unwrap();
        let rec_wrong_owner = IdemRecord {
            schema: IDEM_SCHEMA,
            tenant: "globex".into(),
            slug: victim.slug.clone(),
        };
        std::fs::write(
            store.idem_path("acme", "k1"),
            serde_json::to_vec(&rec_wrong_owner).unwrap(),
        )
        .unwrap();
        let r1 = store
            .publish("acme", "<h1>mine</h1>".into(), None, Some("k1"))
            .unwrap();
        assert_ne!(
            r1.slug, victim.slug,
            "wrong-owner record must not leak B's page"
        );

        // (b) A mapping recording A as owner but pointing at B's slug is rejected by
        // the authoritative page-meta ownership check.
        let rec_forged = IdemRecord {
            schema: IDEM_SCHEMA,
            tenant: "acme".into(),
            slug: victim.slug.clone(),
        };
        std::fs::write(
            store.idem_path("acme", "k2"),
            serde_json::to_vec(&rec_forged).unwrap(),
        )
        .unwrap();
        let r2 = store
            .publish("acme", "<h1>mine2</h1>".into(), None, Some("k2"))
            .unwrap();
        assert_ne!(
            r2.slug, victim.slug,
            "forged mapping must not leak B's page"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    // --- B2 multi-round live overlay ---------------------------------------

    #[test]
    fn push_round_advances_served_body_and_round_without_touching_baseline() {
        let root = tmp_root("round");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let p = store
            .publish("acme", "<h1>round zero</h1>".into(), None, None)
            .unwrap();
        let baseline_path = root.join("pages").join(&p.slug).join("artifact.html");
        let baseline_before = std::fs::read_to_string(&baseline_path).unwrap();

        // First push → round 1, served body swapped, new content-version.
        let r1 = store
            .push_round("acme", &p.slug, "<h1>round one</h1>".into())
            .unwrap();
        assert_eq!(r1.round, 1);
        assert_eq!(
            r1.content_version,
            crate::submissions::content_version("<h1>round one</h1>")
        );
        assert_eq!(
            store.page_body(&p.slug, None).as_deref(),
            Some("<h1>round one</h1>")
        );

        // Second push → monotonic round 2.
        let r2 = store
            .push_round("acme", &p.slug, "<h1>round two</h1>".into())
            .unwrap();
        assert_eq!(r2.round, 2);
        assert_eq!(
            store.page_body(&p.slug, None).as_deref(),
            Some("<h1>round two</h1>")
        );

        // The immutable baseline artifact.html was NEVER rewritten.
        assert_eq!(
            std::fs::read_to_string(&baseline_path).unwrap(),
            baseline_before,
            "push_round must not rewrite the immutable baseline"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn live_round_survives_reopen_and_gc_rescan() {
        // The overlay is durable: after reopening the store the served body is still
        // the latest round (not the baseline), and the round counter continues
        // monotonically — reconnect/replay resumes at the current round.
        let root = tmp_root("round-durable");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let p = store
            .publish("acme", "<h1>base</h1>".into(), None, None)
            .unwrap();
        store
            .push_round("acme", &p.slug, "<h1>live</h1>".into())
            .unwrap();
        drop(store);

        let h2 = host();
        let store2 = Store::open(&root, h2.clone()).unwrap();
        assert_eq!(
            store2.page_body(&p.slug, None).as_deref(),
            Some("<h1>live</h1>"),
            "reopened store must serve the live round, not the baseline"
        );
        // Next push is round 2 (counter recovered from the durable overlay).
        let r = store2
            .push_round("acme", &p.slug, "<h1>next</h1>".into())
            .unwrap();
        assert_eq!(r.round, 2);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn push_round_is_owner_scoped() {
        // Another tenant cannot re-render a page it does not own — opaque 404-shaped
        // error, and the victim's served body is unchanged.
        let root = tmp_root("round-owner");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let victim = store
            .publish("globex", "<h1>owned by globex</h1>".into(), None, None)
            .unwrap();
        let err = store
            .push_round("acme", &victim.slug, "<h1>hijacked</h1>".into())
            .unwrap_err();
        assert!(matches!(err, RoundError::NoSuchPage));
        assert_eq!(
            store.page_body(&victim.slug, None).as_deref(),
            Some("<h1>owned by globex</h1>"),
            "a non-owner push must not change the served body"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn push_round_to_unknown_page_is_no_such_page() {
        let root = tmp_root("round-404");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let err = store
            .push_round("acme", "aaaaaaaaaaaaaaaaaaaaaaaaaa", "<h1>x</h1>".into())
            .unwrap_err();
        assert!(matches!(err, RoundError::NoSuchPage));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn corrupt_live_overlay_falls_back_to_baseline() {
        // A hand-tampered `live.json` (bad schema) must not blank or break the page —
        // it reverts to the immutable baseline.
        let root = tmp_root("round-corrupt");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let p = store
            .publish("acme", "<h1>baseline</h1>".into(), None, None)
            .unwrap();
        store
            .push_round("acme", &p.slug, "<h1>live</h1>".into())
            .unwrap();
        // Corrupt the overlay meta on disk, then reopen.
        std::fs::write(
            root.join("pages").join(&p.slug).join("live.json"),
            b"not json",
        )
        .unwrap();
        let h2 = host();
        let store2 = Store::open(&root, h2.clone()).unwrap();
        assert_eq!(
            store2.page_body(&p.slug, None).as_deref(),
            Some("<h1>baseline</h1>"),
            "a corrupt overlay must revert to the baseline, never blank the page"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn crash_torn_overlay_is_rejected_not_served_under_wrong_round() {
        // Simulate a crash between the live.html and live.json renames: live.html holds
        // a NEW body but live.json still describes the OLD round/content-version. The
        // digest cross-check must reject the mismatched pair and revert to baseline —
        // never serve the new body under the stale round label.
        let root = tmp_root("round-torn");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let p = store
            .publish("acme", "<h1>baseline</h1>".into(), None, None)
            .unwrap();
        store
            .push_round("acme", &p.slug, "<h1>committed round</h1>".into())
            .unwrap();
        // Overwrite ONLY live.html with a different body (live.json still points at the
        // committed round's digest) — the torn state a crash-after-body-rename leaves.
        std::fs::write(
            root.join("pages").join(&p.slug).join("live.html"),
            b"<h1>uncommitted body</h1>",
        )
        .unwrap();
        let h2 = host();
        let store2 = Store::open(&root, h2.clone()).unwrap();
        assert_eq!(
            store2.page_body(&p.slug, None).as_deref(),
            Some("<h1>baseline</h1>"),
            "a torn overlay (body≠meta digest) must revert to baseline"
        );
        // A subsequent push recovers cleanly (round re-derived over the baseline).
        let r = store2
            .push_round("acme", &p.slug, "<h1>recovered</h1>".into())
            .unwrap();
        assert_eq!(r.round, 1);
        assert_eq!(
            store2.page_body(&p.slug, None).as_deref(),
            Some("<h1>recovered</h1>")
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn gc_reclaims_dangling_idem_mappings() {
        // Once a page is GC'd its mapping is dead weight; GC removes it so the idem
        // tree stays bounded to live pages.
        let root = tmp_root("idem-gc");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let p = store
            .publish("acme", "<h1>x</h1>".into(), None, Some("k"))
            .unwrap();
        let mapping = store.idem_path("acme", "k");
        assert!(mapping.is_file(), "mapping written");

        // Backdate + GC the page.
        let meta_path = root.join("pages").join(&p.slug).join("meta.json");
        let mut meta: PageMeta =
            serde_json::from_slice(&std::fs::read(&meta_path).unwrap()).unwrap();
        meta.created_at = Utc::now() - Duration::days(100);
        std::fs::write(&meta_path, serde_json::to_vec_pretty(&meta).unwrap()).unwrap();
        assert_eq!(store.gc(Duration::days(90)).unwrap(), 1);

        assert!(
            !mapping.exists(),
            "GC must reclaim the now-dangling idem mapping"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    // --- Gap 1: multi-artifact spaces --------------------------------------

    fn sample_space(home_title: &str) -> Space {
        build_space_bundle(
            vec![
                BundlePage {
                    slug: "index".into(),
                    html: format!(
                        "<title>{home_title}</title><h1>{home_title}</h1><a href=\"./guide\">g</a>"
                    ),
                },
                BundlePage {
                    slug: "guide".into(),
                    html: "<h1>Guide</h1>".into(),
                },
            ],
            vec![BundleAsset {
                path: "logo.svg".into(),
                bytes: b"<svg></svg>".to_vec(),
            }],
            vec!["index".into(), "guide".into()],
            vec![],
            Some("Docs".into()),
        )
        .unwrap()
    }

    #[test]
    fn publish_space_persists_serves_and_survives_reopen() {
        let root = tmp_root("space-persist");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let pubd = store
            .publish_space("acme", sample_space("Home"), None)
            .unwrap();
        assert!(crate::artifact_host::valid_name(&pubd.slug));
        assert!(pubd.created);
        assert_eq!(pubd.pages.len(), 2);

        // Served as a multi-artifact space in the snapshot.
        let snap = h.snapshot();
        let sp = snap.space(&pubd.slug).unwrap();
        assert_eq!(sp.artifacts.len(), 2);
        assert!(sp.artifact("index").is_some());
        assert!(sp.artifact("guide").is_some());
        assert!(sp.asset("assets/logo.svg").is_some());
        assert_eq!(sp.home.as_deref(), Some("index"));

        // On-disk layout.
        let dir = root.join("spaces").join(&pubd.slug);
        assert!(dir.join("meta.json").is_file());
        assert!(dir.join("artifacts/index.html").is_file());
        assert!(dir.join("artifacts/guide.html").is_file());
        assert!(dir.join("assets/logo.svg").is_file());

        // Reopen: space reloads (re-validated) from disk.
        let h2 = host();
        let store2 = Store::open(&root, h2.clone()).unwrap();
        let sp2 = h2.snapshot().space(&pubd.slug).cloned().unwrap();
        assert_eq!(sp2.artifacts.len(), 2);
        assert_eq!(store2.page_count(), 1);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn space_favicon_persists_and_survives_reopen() {
        let root = tmp_root("space-favicon-persist");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let mut space = sample_space("Home");
        space.favicon = Some("🚀".to_string());
        let pubd = store.publish_space("acme", space, None).unwrap();

        // Served snapshot carries the per-space favicon.
        assert_eq!(
            h.snapshot().space(&pubd.slug).unwrap().favicon.as_deref(),
            Some("🚀")
        );
        // Persisted to meta.json.
        let meta_path = root.join("spaces").join(&pubd.slug).join("meta.json");
        let meta: SpaceMeta = serde_json::from_slice(&std::fs::read(&meta_path).unwrap()).unwrap();
        assert_eq!(meta.favicon.as_deref(), Some("🚀"));

        // Reopen: the favicon reloads from disk.
        let h2 = host();
        let _store2 = Store::open(&root, h2.clone()).unwrap();
        assert_eq!(
            h2.snapshot().space(&pubd.slug).unwrap().favicon.as_deref(),
            Some("🚀")
        );

        // Backward compat: a pre-feature meta.json without the field loads as None
        // (`#[serde(default)]`), and a tampered/invalid favicon is dropped to None (the
        // page reverts to the default) rather than skipping the whole space.
        for tamper in ["null", "\"<script>\""] {
            let raw = std::fs::read_to_string(&meta_path).unwrap();
            let mut json: serde_json::Value = serde_json::from_str(&raw).unwrap();
            if tamper == "null" {
                json.as_object_mut().unwrap().remove("favicon");
            } else {
                json["favicon"] = serde_json::json!("<script>");
            }
            std::fs::write(&meta_path, serde_json::to_vec_pretty(&json).unwrap()).unwrap();
            let h3 = host();
            let _s3 = Store::open(&root, h3.clone()).unwrap();
            let sp3 = h3.snapshot().space(&pubd.slug).cloned();
            assert!(sp3.is_some(), "space still served with favicon={tamper}");
            assert_eq!(sp3.unwrap().favicon, None);
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn space_key_updates_in_place_same_slug() {
        let root = tmp_root("space-idem");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let first = store
            .publish_space("acme", sample_space("V1"), Some("docs"))
            .unwrap();
        assert!(first.created);
        // Re-publish with the same key → SAME slug, updated in place, created=false.
        let again = store
            .publish_space("acme", sample_space("V2"), Some("docs"))
            .unwrap();
        assert_eq!(first.slug, again.slug, "same key must reuse the slug");
        assert!(!again.created, "re-publish updates in place");
        assert_eq!(store.page_count(), 1, "no duplicate space");
        // The served home body reflects the NEW content.
        let sp = h.snapshot().space(&first.slug).cloned().unwrap();
        assert!(sp.artifact("index").unwrap().html.contains("V2"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn update_space_replaces_in_place_by_slug_preserving_created_at() {
        // The `--update <slug>` path: publish a space (no key), then replace its
        // content addressed by the returned slug. Same slug/URL, body swaps, the
        // retention clock (`created_at`) is preserved while `updated_at` advances.
        let root = tmp_root("update-inplace");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let first = store
            .publish_space("acme", sample_space("V1"), None)
            .unwrap();
        let slug = first.slug.clone();
        let meta_path = root.join("spaces").join(&slug).join("meta.json");
        let created_before: SpaceMeta =
            serde_json::from_slice(&std::fs::read(&meta_path).unwrap()).unwrap();

        // A tiny sleep so `updated_at` can strictly advance past `created_at`.
        std::thread::sleep(std::time::Duration::from_millis(5));
        let updated = store
            .update_space("acme", &slug, sample_space("V2"))
            .unwrap();
        assert_eq!(updated.slug, slug, "URL/slug preserved");
        assert!(!updated.created, "an update is never a create");
        assert_eq!(store.page_count(), 1, "no duplicate space minted");

        // Served body now reflects V2.
        let sp = h.snapshot().space(&slug).cloned().unwrap();
        assert!(sp.artifact("index").unwrap().html.contains("V2"));

        // created_at preserved, updated_at advanced.
        let meta_after: SpaceMeta =
            serde_json::from_slice(&std::fs::read(&meta_path).unwrap()).unwrap();
        assert_eq!(
            meta_after.created_at, created_before.created_at,
            "retention clock (created_at) must be preserved on update"
        );
        assert!(
            meta_after.updated_at > created_before.updated_at,
            "updated_at must advance on an in-place update"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn update_space_is_owner_scoped_and_fail_if_missing() {
        // Foreign-owned, unknown, and page-tree-only slugs all return the same opaque
        // NoSuchSpace — never a fresh create, no cross-tenant existence oracle.
        let root = tmp_root("update-owner");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let owned = store
            .publish_space("acme", sample_space("mine"), None)
            .unwrap();

        // Another tenant cannot update acme's space (and learns nothing).
        assert!(matches!(
            store.update_space("globex", &owned.slug, sample_space("evil")),
            Err(UpdateError::NoSuchSpace)
        ));
        // acme's space is untouched by the refused foreign update.
        assert!(
            h.snapshot()
                .space(&owned.slug)
                .unwrap()
                .artifact("index")
                .unwrap()
                .html
                .contains("mine")
        );

        // An entirely unknown slug is the same opaque error, NOT a create.
        assert!(matches!(
            store.update_space("acme", "aaaaaaaaaaaaaaaaaaaaaaaaaa", sample_space("x")),
            Err(UpdateError::NoSuchSpace)
        ));
        assert_eq!(store.page_count(), 1, "a missing-slug update never creates");

        // A single-artifact PAGE slug (pages/ tree, not spaces/) is not a space →
        // NoSuchSpace, so `--update` can't clobber the immutable page path.
        let page = store
            .publish("acme", "<h1>page</h1>".into(), None, None)
            .unwrap();
        assert!(matches!(
            store.update_space("acme", &page.slug, sample_space("x")),
            Err(UpdateError::NoSuchSpace)
        ));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn update_space_fails_closed_on_page_space_slug_collision() {
        // A corrupt/tampered store where the SAME slug exists in both trees: the
        // snapshot serves the page (pages load first), but a hand-planted
        // `spaces/<slug>/meta.json` names the requesting tenant. `update_space` must
        // refuse (NoSuchSpace) rather than replace `spaces/<slug>` and leave the page
        // shadowing it — parity with `page_tenant`'s fail-closed stance.
        let root = tmp_root("update-collision");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let page = store
            .publish("acme", "<h1>page</h1>".into(), None, None)
            .unwrap();
        let slug = page.slug.clone();
        // Hand-plant a spaces/<slug>/meta.json owned by acme (a state the API prevents).
        let now = Utc::now();
        let space_dir = root.join("spaces").join(&slug);
        std::fs::create_dir_all(&space_dir).unwrap();
        std::fs::write(
            space_dir.join(META_FILE),
            serde_json::to_vec(&SpaceMeta {
                schema: SPACE_META_SCHEMA,
                slug: slug.clone(),
                tenant: "acme".into(),
                title: None,
                nav: vec![],
                nav_groups: vec![],
                home: None,
                favicon: None,
                created_at: now,
                updated_at: now,
            })
            .unwrap(),
        )
        .unwrap();
        assert!(matches!(
            store.update_space("acme", &slug, sample_space("x")),
            Err(UpdateError::NoSuchSpace)
        ));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn concurrent_update_space_serialize_last_writer_wins_no_duplicate() {
        // Two concurrent PUTs to the same slug are serialized by the mutation lock:
        // both succeed, the store never duplicates the space, and the served body is
        // one of the two writers' (last-writer-wins). No torn/lost snapshot.
        let root = tmp_root("update-concurrent");
        let h = host();
        let store = Arc::new(Store::open(&root, h.clone()).unwrap());
        let first = store
            .publish_space("acme", sample_space("V0"), None)
            .unwrap();
        let slug = first.slug.clone();

        let mut handles = Vec::new();
        for tag in ["A", "B", "C", "D"] {
            let store = store.clone();
            let slug = slug.clone();
            handles.push(std::thread::spawn(move || {
                store
                    .update_space("acme", &slug, sample_space(tag))
                    .map(|p| p.created)
            }));
        }
        for hd in handles {
            // Every concurrent update succeeds and is an update (never a create).
            assert!(!hd.join().unwrap().unwrap());
        }
        assert_eq!(
            store.page_count(),
            1,
            "no duplicate space from concurrent PUTs"
        );
        let served = h
            .snapshot()
            .space(&slug)
            .unwrap()
            .artifact("index")
            .unwrap()
            .html
            .clone();
        assert!(
            ["A", "B", "C", "D"].iter().any(|t| served.contains(t)),
            "served body must be one writer's content, got: {served}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn update_space_survives_reopen() {
        // The replaced content is durable: a fresh store reload serves V2.
        let root = tmp_root("update-reopen");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let first = store
            .publish_space("acme", sample_space("V1"), None)
            .unwrap();
        store
            .update_space("acme", &first.slug, sample_space("V2"))
            .unwrap();

        let h2 = host();
        let store2 = Store::open(&root, h2.clone()).unwrap();
        assert_eq!(store2.page_count(), 1);
        let sp = h2.snapshot().space(&first.slug).cloned().unwrap();
        assert!(sp.artifact("index").unwrap().html.contains("V2"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn space_key_is_scoped_per_tenant() {
        let root = tmp_root("space-idem-tenant");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let a = store
            .publish_space("acme", sample_space("A"), Some("shared"))
            .unwrap();
        let b = store
            .publish_space("globex", sample_space("B"), Some("shared"))
            .unwrap();
        assert_ne!(
            a.slug, b.slug,
            "same key, different tenants → different spaces"
        );
        // Each tenant's repeat still updates its own space.
        let a2 = store
            .publish_space("acme", sample_space("A2"), Some("shared"))
            .unwrap();
        assert_eq!(a.slug, a2.slug);
        assert_eq!(store.page_count(), 2);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn no_space_key_mints_fresh_every_time() {
        let root = tmp_root("space-nokey");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let a = store
            .publish_space("acme", sample_space("A"), None)
            .unwrap();
        let b = store
            .publish_space("acme", sample_space("A"), None)
            .unwrap();
        assert_ne!(a.slug, b.slug);
        assert_eq!(store.page_count(), 2);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn gc_removes_expired_space_and_stops_serving_it() {
        let root = tmp_root("space-gc");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let p = store
            .publish_space("acme", sample_space("X"), None)
            .unwrap();
        // Backdate the space meta past retention (updated_at is the space retention
        // clock — a fresh publish sets it to now, so backdate it, not just created_at).
        let meta_path = root.join("spaces").join(&p.slug).join("meta.json");
        let mut meta: SpaceMeta =
            serde_json::from_slice(&std::fs::read(&meta_path).unwrap()).unwrap();
        meta.created_at = Utc::now() - Duration::days(100);
        meta.updated_at = Utc::now() - Duration::days(100);
        std::fs::write(&meta_path, serde_json::to_vec_pretty(&meta).unwrap()).unwrap();
        let removed = store.gc(Duration::days(90)).unwrap();
        assert_eq!(removed, 1);
        assert!(!root.join("spaces").join(&p.slug).exists());
        assert!(h.snapshot().space(&p.slug).is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn cross_tenant_space_key_mapping_is_never_honored() {
        // A hand-forged mapping under tenant A naming a space owned by B must not let
        // A overwrite B's space: A gets its own fresh space.
        let root = tmp_root("space-crosstenant");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let victim = store
            .publish_space("globex", sample_space("secret"), None)
            .unwrap();
        // Forge a mapping under acme pointing at globex's slug, recording acme owner.
        let tenant_dir = root.join("space-idem").join("acme");
        std::fs::create_dir_all(&tenant_dir).unwrap();
        let forged = IdemRecord {
            schema: IDEM_SCHEMA,
            tenant: "acme".into(),
            slug: victim.slug.clone(),
        };
        std::fs::write(
            store.space_idem_path("acme", "k"),
            serde_json::to_vec(&forged).unwrap(),
        )
        .unwrap();
        let mine = store
            .publish_space("acme", sample_space("mine"), Some("k"))
            .unwrap();
        assert_ne!(
            mine.slug, victim.slug,
            "forged mapping must not target B's space"
        );
        // The victim's space is unchanged.
        let sp = h.snapshot().space(&victim.slug).cloned().unwrap();
        assert!(sp.artifact("index").unwrap().html.contains("secret"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn interrupted_replace_is_recovered_from_backup_on_reopen() {
        // Simulate a crash BETWEEN the two renames of an in-place update: the live
        // `spaces/<slug>` is gone and the only copy is in `.<slug>.old`. Reopening the
        // store must RESTORE it (never lose the last committed generation), and GC must
        // not reap the backup before recovery.
        let root = tmp_root("space-recover");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let p = store
            .publish_space("acme", sample_space("committed"), Some("k"))
            .unwrap();
        // Move the live dir aside to `.<slug>.old` (the torn mid-replace state).
        let final_dir = root.join("spaces").join(&p.slug);
        let backup = root.join("spaces").join(format!(".{}.old", p.slug));
        std::fs::rename(&final_dir, &backup).unwrap();
        assert!(!final_dir.exists());

        // Reopen: recovery restores the space from the backup.
        let h2 = host();
        let store2 = Store::open(&root, h2.clone()).unwrap();
        assert!(final_dir.exists(), "recovery must restore final from .old");
        assert!(!backup.exists(), "the backup is consumed by recovery");
        let sp = h2.snapshot().space(&p.slug).cloned().unwrap();
        assert!(sp.artifact("index").unwrap().html.contains("committed"));
        assert_eq!(store2.page_count(), 1);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn space_retention_is_measured_from_last_update_not_first_publish() {
        // An actively re-published space keeps its lease: even though created_at is
        // old, a recent updated_at means GC keeps it. A stale updated_at expires it.
        let root = tmp_root("space-retention");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let p = store
            .publish_space("acme", sample_space("v1"), Some("k"))
            .unwrap();
        let meta_path = root.join("spaces").join(&p.slug).join("meta.json");
        // First published long ago, updated moments ago (an active docsite).
        let mut meta: SpaceMeta =
            serde_json::from_slice(&std::fs::read(&meta_path).unwrap()).unwrap();
        meta.created_at = Utc::now() - Duration::days(100);
        meta.updated_at = Utc::now();
        std::fs::write(&meta_path, serde_json::to_vec_pretty(&meta).unwrap()).unwrap();
        assert_eq!(
            store.gc(Duration::days(90)).unwrap(),
            0,
            "an actively-updated space must NOT be GC'd on its old created_at"
        );
        assert!(h.snapshot().space(&p.slug).is_some());

        // Now make updated_at stale too → it expires.
        let mut meta: SpaceMeta =
            serde_json::from_slice(&std::fs::read(&meta_path).unwrap()).unwrap();
        meta.updated_at = Utc::now() - Duration::days(100);
        std::fs::write(&meta_path, serde_json::to_vec_pretty(&meta).unwrap()).unwrap();
        assert_eq!(store.gc(Duration::days(90)).unwrap(), 1);
        assert!(h.snapshot().space(&p.slug).is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn replace_commits_and_swaps_even_when_post_commit_fsync_fails() {
        // The fsync-after-swap divergence window (issue: materialize-space-durability).
        // The publishing rename has already put the new tree in `spaces/<slug>/`, then
        // the parent-dir fsync fails. The update must still COMMIT: return Ok, swap the
        // served snapshot to the new content, and leave no `.old`/`.tmp` divergence — so
        // a caller can never treat "new tree already final on disk" as "unchanged".
        // (Before the fix this returned Err, the snapshot stayed V1, and a restart —
        // which rebuilds from disk — served V2: memory/disk divergence until restart.)
        let root = tmp_root("materialize-fsync-replace");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        let first = store
            .publish_space("acme", sample_space("V1"), None)
            .unwrap();
        let slug = first.slug.clone();

        // Arm the next post-commit fsync (this thread) to fail, then update in place.
        fault::arm_commit_fsync_faults(1);
        let updated = store
            .update_space("acme", &slug, sample_space("V2"))
            .expect("a post-commit fsync failure must not fail the committed update");
        assert_eq!(updated.slug, slug);
        assert!(!updated.created);
        assert_eq!(store.page_count(), 1, "no duplicate space");

        // Memory (served snapshot) matches the committed disk tree: both are V2.
        let sp = h.snapshot().space(&slug).cloned().unwrap();
        assert!(
            sp.artifact("index").unwrap().html.contains("V2"),
            "served snapshot must be swapped to the committed new tree"
        );
        // No stranded staging dirs the caller/recovery would have to reconcile.
        assert!(!root.join("spaces").join(format!(".{slug}.old")).exists());
        assert!(!root.join("spaces").join(format!(".{slug}.tmp")).exists());

        // Disk truth agrees on reopen (rescan serves V2, page_count 1) — the same
        // content the running process serves, i.e. no restart-time divergence.
        let h2 = host();
        let store2 = Store::open(&root, h2.clone()).unwrap();
        assert_eq!(store2.page_count(), 1);
        assert!(
            h2.snapshot()
                .space(&slug)
                .unwrap()
                .artifact("index")
                .unwrap()
                .html
                .contains("V2")
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn fresh_create_commits_and_swaps_even_when_post_commit_fsync_fails() {
        // Same divergence window on the CREATE path: the fresh tree's rename has
        // committed, then the parent fsync fails. publish_space must still return Ok and
        // serve the new space (the caller holds the returned slug), not surface an error
        // and strand an orphan tree the client never learns about.
        let root = tmp_root("materialize-fsync-create");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();

        fault::arm_commit_fsync_faults(1);
        let pubd = store
            .publish_space("acme", sample_space("Fresh"), None)
            .expect("a post-commit fsync failure must not fail the committed create");
        assert!(pubd.created);
        assert_eq!(store.page_count(), 1);

        // Served immediately (snapshot swapped despite the flush failure).
        let sp = h.snapshot().space(&pubd.slug).cloned().unwrap();
        assert!(sp.artifact("index").unwrap().html.contains("Fresh"));
        assert!(
            !root
                .join("spaces")
                .join(format!(".{}.tmp", pubd.slug))
                .exists()
        );

        // And durable on reopen (memory and disk agree).
        let h2 = host();
        let _store2 = Store::open(&root, h2.clone()).unwrap();
        assert!(
            h2.snapshot()
                .space(&pubd.slug)
                .unwrap()
                .artifact("index")
                .unwrap()
                .html
                .contains("Fresh")
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn post_commit_fsync_failure_preserves_idempotency_and_ownership() {
        // The commit-invariant fix must not weaken keyed-replay or tenant isolation.
        // A keyed publish whose post-commit fsync fails still commits, and a later
        // same-key publish replays the SAME slug in place (never a duplicate); a foreign
        // tenant's key is scoped to its own space, and an unknown slug still fails closed.
        let root = tmp_root("materialize-fsync-idem");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();

        fault::arm_commit_fsync_faults(1);
        let first = store
            .publish_space("acme", sample_space("V1"), Some("docs"))
            .expect("keyed create commits through a post-commit fsync failure");
        assert!(first.created);

        // Same key → same slug, updated in place (idempotency-key replay intact).
        let again = store
            .publish_space("acme", sample_space("V2"), Some("docs"))
            .unwrap();
        assert_eq!(first.slug, again.slug, "keyed replay must reuse the slug");
        assert!(!again.created);
        assert_eq!(store.page_count(), 1, "no duplicate from the replayed key");
        assert!(
            h.snapshot()
                .space(&first.slug)
                .unwrap()
                .artifact("index")
                .unwrap()
                .html
                .contains("V2")
        );

        // Tenant isolation: a foreign tenant's same key mints its OWN space, and an
        // unknown-slug update still fails closed (no cross-tenant existence oracle).
        let other = store
            .publish_space("globex", sample_space("B"), Some("docs"))
            .unwrap();
        assert_ne!(
            first.slug, other.slug,
            "same key, different tenant → different space"
        );
        assert!(matches!(
            store.update_space("globex", &first.slug, sample_space("evil")),
            Err(UpdateError::NoSuchSpace)
        ));
        std::fs::remove_dir_all(&root).ok();
    }
}
