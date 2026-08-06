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
//! Pages are **immutable**: a page is written once (to a temp path then atomically
//! `rename`d in, so a crash never exposes a half-page) and never mutated — there is
//! no update/overwrite/delete API, and `publish` always mints a **fresh random
//! slug**, so one tenant can never overwrite another's page. The in-memory
//! [`Snapshot`] the read handlers serve is the serving source of truth; the on-disk
//! tree is the persistence + GC source of truth.
//!
//! Retention/GC removes page directories older than the retention window and swaps
//! a rebuilt snapshot so an expired page is promptly **no longer served**.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::artifact_host::ArtifactHost;
use crate::artifact_host::space::{self, Artifact, Snapshot, Space};
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

/// The persistent page store. Holds the storage root and a handle to the
/// [`ArtifactHost`] whose snapshot it keeps in sync on publish + GC.
pub struct Store {
    pages_dir: PathBuf,
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
        let store = Store {
            pages_dir,
            host,
            mutation: std::sync::Mutex::new(()),
        };
        let snap = store.scan_disk();
        store.host.swap(snap);
        Ok(store)
    }

    /// The number of pages currently served (snapshot space count).
    pub fn page_count(&self) -> usize {
        self.host.snapshot().spaces.len()
    }

    /// Scan the whole `pages/` tree into a fresh [`Snapshot`]. Each subdirectory is
    /// one page; unreadable/corrupt/oversize/invalid pages are skipped + logged.
    fn scan_disk(&self) -> Snapshot {
        let mut snap = Snapshot::empty();
        let rd = match std::fs::read_dir(&self.pages_dir) {
            Ok(rd) => rd,
            Err(e) => {
                eprintln!(
                    "glasspad host: cannot read pages dir {}: {e}",
                    self.pages_dir.display()
                );
                return snap;
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
        snap
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
        let html = read_capped_utf8(&art_path)?;
        Ok(Some((
            meta.clone(),
            one_artifact_space(html, meta.title.clone()),
        )))
    }

    /// Publish one immutable page for `tenant`. Generates a fresh unguessable slug
    /// (regenerating on the astronomically-unlikely collision), writes the artifact
    /// and meta atomically, and inserts the new single-artifact space into the live
    /// snapshot via the host's atomic swap. Returns the minted slug and resolved
    /// title. Enforces [`MAX_PAGES`].
    pub fn publish(
        &self,
        tenant: &str,
        html: String,
        title_override: Option<String>,
    ) -> Result<Published, PublishError> {
        // Serialize the whole read→write→swap window so concurrent publishes (and
        // GC) can't lose each other's snapshot updates. Held across the blocking
        // disk write; ingest calls this on a blocking thread (`spawn_blocking`).
        let _guard = self.mutation.lock().expect("store mutation lock poisoned");
        let current = self.host.snapshot();
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

        // Clone-modify-swap: readers in flight keep the old Arc; new readers see the
        // added page. (O(n) rebuild is acceptable at this iteration's scale; a
        // future Arc-shared Space body would make it O(1) — see plan §6.)
        let mut spaces = current.spaces.clone();
        spaces.insert(slug.clone(), one_artifact_space(html, title.clone()));
        self.host.swap(Snapshot { spaces });

        Ok(Published { slug, title })
    }

    /// A slug present in neither the current snapshot nor on disk. Bounded retries
    /// (a 128-bit collision is effectively impossible; the loop is belt-and-braces).
    fn fresh_slug(&self, current: &Snapshot) -> Result<String, PublishError> {
        for _ in 0..8 {
            let s = slug::generate();
            if !current.spaces.contains_key(&s) && !self.pages_dir.join(&s).exists() {
                return Ok(s);
            }
        }
        Err(PublishError::SlugExhausted)
    }

    /// Atomically materialize a page directory: write both files into a `.<slug>.tmp`
    /// staging dir, then `rename` it into place, so a reader never sees a page dir
    /// that exists but lacks its artifact/meta.
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
            std::fs::write(tmp_dir.join(ARTIFACT_FILE), html)?;
            let meta_json = serde_json::to_vec_pretty(meta)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            std::fs::write(tmp_dir.join(META_FILE), meta_json)?;
            // rename is atomic on the same filesystem; the staging dir is a sibling
            // so it shares the pages_dir filesystem.
            std::fs::rename(&tmp_dir, &final_dir)
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
    /// matches what survives on disk (no window serving a deleted page).
    pub fn gc(&self, retention: Duration) -> std::io::Result<usize> {
        // Same lock as `publish`: GC's scan+swap must not race a publish's swap
        // (which would let GC's rebuilt snapshot clobber a just-published page, or
        // a stale publish resurrect a page GC just deleted).
        let _guard = self.mutation.lock().expect("store mutation lock poisoned");
        let cutoff = Utc::now() - retention;
        let mut removed = 0usize;
        let rd = std::fs::read_dir(&self.pages_dir)?;
        for entry in rd.flatten() {
            let dir = entry.path();
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
        if removed > 0 {
            // Rebuild the served snapshot from what remains on disk.
            let snap = self.scan_disk();
            self.host.swap(snap);
        }
        Ok(removed)
    }

    fn read_created_at(&self, dir: &Path) -> Option<DateTime<Utc>> {
        let bytes = read_capped(&dir.join(META_FILE), MAX_META_BYTES).ok()?;
        if bytes.len() as u64 > MAX_META_BYTES {
            return None;
        }
        let meta: PageMeta = serde_json::from_slice(&bytes).ok()?;
        Some(meta.created_at)
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
    fn publish_persists_and_serves_and_survives_reopen() {
        let root = tmp_root("persist");
        let h = host();
        let store = Store::open(&root, h.clone()).unwrap();
        assert_eq!(store.page_count(), 0);

        let pub1 = store
            .publish("acme", "<h1>Hello</h1>".into(), None)
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
                    .publish("acme", format!("<h1>page {i}</h1>"), None)
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
        let p = store.publish("globex", "<h1>x</h1>".into(), None).unwrap();
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
        let a = store.publish("acme", "<h1>A</h1>".into(), None).unwrap();
        let b = store.publish("acme", "<h1>B</h1>".into(), None).unwrap();
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
        let p = store.publish("acme", "<h1>old</h1>".into(), None).unwrap();

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
        let p = store.publish("acme", "<h1>new</h1>".into(), None).unwrap();
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
            .publish("acme", "<title>FromHtml</title><h1>x</h1>".into(), None)
            .unwrap();
        assert_eq!(a.title, "FromHtml");
        let b = store
            .publish("acme", "<h1>x</h1>".into(), Some("Override".into()))
            .unwrap();
        assert_eq!(b.title, "Override");
        std::fs::remove_dir_all(&root).ok();
    }
}
