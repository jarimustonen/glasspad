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
//! no update/overwrite/delete API, and `publish` mints a **fresh random slug** for
//! every new page, so one tenant can never overwrite another's page. The in-memory
//! [`Snapshot`] the read handlers serve is the serving source of truth; the on-disk
//! tree is the persistence + GC source of truth.
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
    /// Root of the per-tenant idempotency-mapping tree (`<root>/idem/<tenant>/`).
    /// A tenant's mappings live only under its own subdirectory, so a key lookup
    /// is scoped to the authenticated tenant by construction.
    idem_dir: PathBuf,
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

impl Store {
    /// Open (creating if needed) the store rooted at `root`, load every existing
    /// page into the host's initial snapshot, and return the store. A corrupt or
    /// oversize individual page is **skipped with a log line**, not fatal — one bad
    /// directory must not stop the whole server coming up. Returns the loaded page
    /// count via [`Store::page_count`] after open.
    pub fn open(root: &Path, host: Arc<ArtifactHost>) -> std::io::Result<Self> {
        let pages_dir = root.join("pages");
        std::fs::create_dir_all(&pages_dir)?;
        let idem_dir = root.join("idem");
        std::fs::create_dir_all(&idem_dir)?;
        // Persist the `pages`/`idem` directory entries themselves before any write
        // relies on them being present after a crash (fsync of a child dir does not
        // persist the child's entry in its parent).
        fsync_dir(root)?;
        let store = Store {
            pages_dir,
            idem_dir,
            host,
            mutation: std::sync::Mutex::new(()),
        };
        let snap = store.scan_disk();
        store.host.swap(snap);
        Ok(store)
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
    /// if the page is absent/unreadable or its meta names a different slug. This is
    /// the authority the return-channel read scoping uses: a tenant may read a
    /// page's submissions only when this equals its authenticated id.
    pub fn page_tenant(&self, slug: &str) -> Option<String> {
        let meta_path = self.pages_dir.join(slug).join(META_FILE);
        let bytes = read_capped(&meta_path, MAX_META_BYTES).ok()?;
        if bytes.len() as u64 > MAX_META_BYTES {
            return None;
        }
        serde_json::from_slice::<PageMeta>(&bytes)
            .ok()
            .filter(|m| m.slug == slug)
            .map(|m| m.tenant)
    }

    /// The currently-served artifact body for `slug` (the single-artifact page's
    /// `index`), or `None` if the page is not served. Used to compute the artifact
    /// content-version a submission answered — server-side, never from the payload.
    pub fn page_body(&self, slug: &str) -> Option<String> {
        self.host
            .snapshot()
            .space(slug)?
            .artifact(SINGLE_SLUG)
            .map(|a| a.html.clone())
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
        let tenant_dir = self.idem_dir.join(tenant);
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
                fsync_dir(&self.idem_dir)?;
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
            if !current.spaces.contains_key(&s) && !self.pages_dir.join(&s).exists() {
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
        if removed > 0 {
            // Make the removals durable, then rebuild + swap the served snapshot from
            // what remains on disk.
            fsync_dir(&self.pages_dir)?;
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
        let tenants = match std::fs::read_dir(&self.idem_dir) {
            Ok(rd) => rd,
            Err(e) => {
                eprintln!(
                    "glasspad host: GC cannot read idem dir {}: {e}",
                    self.idem_dir.display()
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
}
