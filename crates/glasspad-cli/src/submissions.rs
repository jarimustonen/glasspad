//! The **return-channel submission store** — durable per-key submissions plus the
//! long-poll primitive that backs `glasspad await-submission`.
//!
//! A submission is one blob of user input an interactive artifact sent *back* to
//! the agent that authored it (a form answer, a button choice, a wizard step). The
//! data path is the airlock design (`issues/artifact-return-channel/design.md`):
//!
//! ```text
//! [artifact JS]  --postMessage({type:"submit", data})-->  [trusted shell]
//!   connect-src 'none'                                      connect-src 'self'
//!        \--------------------------------------------------------/
//!                              |  POST (shell → server)
//!                              v
//!                     [SubmissionStore]  --long-poll/poll-->  [agent]
//! ```
//!
//! The artifact never gains network egress; the **shell** is the only thing that
//! POSTs, and this store is what it POSTs into. Everything here treats the payload
//! as **untrusted data**: it is size-capped, structurally bounded, persisted as an
//! opaque JSON value, and never `eval`/interpolated. The two anti-spoof invariants
//! are enforced by the *callers* (the HTTP handlers), not here: the addressing
//! `key` (page slug / space) and the owning `tenant` are taken from the trusted
//! request context, never from the submitted payload.
//!
//! ## On-disk layout (mirrors `hosted::store`'s durability contract)
//!
//! ```text
//! <root>/<key>/<id>.json      one submission (fsync'd, atomically renamed in)
//! ```
//!
//! A submission is written to a `.<id>.tmp` sibling, fsync'd, `rename`d into place,
//! and the containing directory fsync'd — so a reader (or a restarted process)
//! never sees a half-written record, and a submission is **durable before it is
//! acknowledged/returned** to any waiter. `id` is a process-global monotonic
//! integer recovered as `max(on-disk id) + 1` on open, so a crash loses no ordering
//! and a cursor (`since=<id>`) never re-delivers or skips a persisted submission.
//!
//! ## Delivery
//!
//! [`list_since`] is the plain-poll substrate (A1): every submission for `key` with
//! `id > since`. [`wait`] is the server-side long-poll (A3): it holds until a
//! submission after the cursor lands or the timeout fires, bounded by a global
//! waiter cap so held connections can never grow without limit. [`open_stream`] is
//! the server-push transport (A2): it holds an SSE connection and pushes each
//! submission after the cursor as it lands, reusing the *same* keyed broadcast as
//! `wait` but a **separate** held-connection budget ([`MAX_STREAM_WAITERS`] +
//! [`MAX_STREAMS_PER_KEY`], so indefinitely-held streams never starve the long-poll).
//! The same `since=<id>` cursor guarantees (no re-deliver, no skip) apply to all three.

use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use axum::response::sse::{Event, KeepAlive, Sse};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::{Stream, StreamExt};

use crate::artifact_host::valid_space;
use glasspad::security::token;

/// Schema guard for the on-disk submission record.
const SUBMISSION_SCHEMA: u32 = 1;

/// Upper bound on the serialized `data` payload of one submission. A submission is
/// a form answer / choice, not a file upload — a larger body is rejected rather
/// than persisted (AI-first strict validation; bounds disk + memory per record).
pub const MAX_SUBMISSION_BYTES: usize = 64 * 1024;

/// Read cap for a single on-disk record (the small JSON envelope around `data`).
/// A larger file is hand-tampered/corrupt and is skipped rather than buffered.
const MAX_RECORD_BYTES: u64 = MAX_SUBMISSION_BYTES as u64 + 16 * 1024;

/// Hard ceiling on stored submissions per key — a backstop against a hostile
/// artifact filling the disk one accepted submission at a time (the rate limiter
/// bounds the *rate*; this bounds the *total*).
pub const MAX_SUBMISSIONS_PER_KEY: usize = 10_000;

/// Longest a long-poll may be held. A client asking for more is clamped to this,
/// so a held connection is always bounded regardless of the requested timeout.
pub const MAX_WAIT_SECS: u64 = 300;

/// Default long-poll hold when the caller does not specify one.
pub const DEFAULT_WAIT_SECS: u64 = 30;

/// Most submissions returned by one poll/wait response (bounds the response size
/// and the per-request read work; the caller advances its cursor and polls again).
pub const MAX_LIST: usize = 100;

/// Maximum concurrently-held long-poll waiters across the whole store. Past this,
/// `wait` returns [`WaitOutcome::TooBusy`] immediately rather than holding another
/// connection — the resource bound the design's quality bar (d) requires.
pub const MAX_WAITERS: usize = 256;

/// Maximum concurrently-held **SSE streams** across the whole store. Streams get a
/// budget **separate** from [`MAX_WAITERS`] (they are held indefinitely, unlike the
/// time-bounded long-poll): a flood of streams can therefore never consume the
/// long-poll budget, so the primary `await-submission` surface keeps its full
/// headroom regardless of stream load. Past this, `open_stream` returns `None` (the
/// caller answers "too busy" → fall back to polling).
pub const MAX_STREAM_WAITERS: usize = 128;

/// Maximum concurrently-held SSE streams for a **single key** (page slug / space).
/// Bounds a single page from monopolizing the stream budget — the fairness half of
/// the [`MAX_STREAM_WAITERS`] cap for the "watch many pages" use case.
pub const MAX_STREAMS_PER_KEY: usize = 8;

/// Sliding-window rate limit on **accepted submits** per key.
const RATE_MAX: usize = 30;
const RATE_WINDOW: Duration = Duration::from_secs(60);
/// Cap on the number of distinct keys the rate-limiter tracks, so a flood of
/// one-off keys can't grow the map without bound (a full map rejects new keys
/// until the sweep below prunes emptied entries — fail-closed on the rate path).
const RATE_MAX_KEYS: usize = 4096;
/// Fixed lock striping keeps collection rewrites and GC serialized for the same
/// key without making one page's disk work block every tenant in the process.
const RECORD_LOCK_SHARDS: usize = 64;

/// One persisted submission. `key`/`tenant`/`content_version` are all set from the
/// **trusted request context** by the handler, never from the artifact payload;
/// `data` is the untrusted payload, stored verbatim as an opaque JSON value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Submission {
    schema: u32,
    /// Process-global monotonic id; the cursor (`since`) is compared against it.
    pub id: u64,
    /// Addressing token: the page slug (hosted) or space name (loopback).
    pub key: String,
    /// The artifact slug within the space the user actually answered (`index` for
    /// a hosted single-artifact page).
    pub artifact: String,
    /// The owning tenant (hosted) or the loopback sentinel. Internal — never
    /// echoed in the public API response.
    pub tenant: String,
    /// The content-version of the artifact this submission answered (server-
    /// computed from the served body — see [`content_version`]). Cross-round hook.
    pub content_version: String,
    pub created_at: DateTime<Utc>,
    /// Unguessable capability returned only to the submitting shell. It is omitted
    /// from the agent-facing public view, so a status read identifies exactly one
    /// browser submission rather than becoming a page-wide read capability.
    #[serde(default)]
    status_token: String,
    /// Set durably when an owner-scoped poll, wait, drain, or stream selects this
    /// record for delivery. Legacy records default to `false` and have no status
    /// token, so they remain readable by agents without gaining a public handle.
    #[serde(default)]
    collected: bool,
    /// The untrusted user payload.
    pub data: serde_json::Value,
}

impl Submission {
    /// The opaque capability the trusted shell uses to read only this submission's
    /// delivery state. Never include it in [`Self::to_public_json`].
    pub fn status_token(&self) -> &str {
        &self.status_token
    }

    /// The public API view of a submission — omits the internal `schema`/`tenant`.
    pub fn to_public_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "key": self.key,
            "artifact": self.artifact,
            "content_version": self.content_version,
            "created_at": self.created_at,
            "data": self.data,
        })
    }
}

/// Why a submit was rejected (mapped to an HTTP status + stable code by the caller).
#[derive(Debug)]
pub enum SubmitError {
    /// The serialized payload exceeds [`MAX_SUBMISSION_BYTES`].
    TooLarge,
    /// The per-key stored-submission cap is reached.
    Full,
    /// The per-key rate limit is exceeded.
    RateLimited,
    Io(std::io::Error),
}

impl std::fmt::Display for SubmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SubmitError::TooLarge => write!(
                f,
                "submission payload exceeds the {MAX_SUBMISSION_BYTES}-byte limit"
            ),
            SubmitError::Full => write!(
                f,
                "this page has reached the {MAX_SUBMISSIONS_PER_KEY}-submission limit"
            ),
            SubmitError::RateLimited => write!(f, "too many submissions; slow down and retry"),
            SubmitError::Io(e) => write!(f, "storage error: {e}"),
        }
    }
}

/// The content-version of an artifact body: the first 16 hex chars of its SHA-256.
/// Stable for a given body (hosted pages are immutable; a loopback reload changes
/// the body and thus the version), short enough for a URL/echo, and collision-
/// resistant enough to bind a submission to the exact content it answered. This is
/// the forward-compat field: one-shot ignores it beyond the mismatch check; a
/// later multi-round protocol keys rounds off it.
pub fn content_version(body: &str) -> String {
    let digest = Sha256::digest(body.as_bytes());
    let mut out = String::with_capacity(16);
    for b in &digest[..8] {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// The durable submission store for one run mode.
pub struct SubmissionStore {
    root: PathBuf,
    /// Next id to allocate (recovered as `max(on-disk) + 1` on open). Atomic so
    /// concurrent submits get distinct ids without a lock; a gap from a crash
    /// between allocation and write is harmless (ids need be unique + monotonic,
    /// not gapless).
    next_id: AtomicU64,
    /// Fired after each durable submit, carrying the submission's `key` so a held
    /// [`wait`] wakes only for its own key (no store-wide thundering herd).
    tx: broadcast::Sender<Arc<str>>,
    /// Count of currently-held long-poll waiters (bounded by [`MAX_WAITERS`]).
    waiters: AtomicUsize,
    /// Count of currently-held SSE streams (bounded by [`MAX_STREAM_WAITERS`],
    /// separate from `waiters` so streams cannot starve the long-poll budget).
    stream_waiters: AtomicUsize,
    /// Per-key held-stream counts (bounded by [`MAX_STREAMS_PER_KEY`]). Entries are
    /// removed when a key's count returns to zero, so the map stays bounded to keys
    /// with live streams.
    stream_per_key: Mutex<HashMap<String, usize>>,
    /// Per-key sliding-window submit timestamps for the rate limiter.
    rate: Mutex<HashMap<String, VecDeque<Instant>>>,
    /// Key-striped synchronization for record creation, collection marking, and
    /// GC. Atomic record reads (including exact status reads) need no lock: they see
    /// either the old or renamed new file, while NotFound means status unavailable.
    record_locks: [Mutex<()>; RECORD_LOCK_SHARDS],
}

impl SubmissionStore {
    /// Open (creating if needed) the store rooted at `root`, recovering the id
    /// counter as the **max of** the highest id on disk and a persisted high-water
    /// mark. The high-water file is what keeps ids monotonic even after GC deletes
    /// every record (which would leave `scan_max_id` at 0): without it, `next_id`
    /// would reset to 1 and a long-lived cursor (`since=N`) would silently skip every
    /// new submission. The persisted value can only ever raise the counter, never
    /// lower it, so a lost/corrupt high-water file falls back safely to the on-disk
    /// scan.
    pub fn open(root: &Path) -> std::io::Result<Arc<Self>> {
        std::fs::create_dir_all(root)?;
        fsync_dir(root)?;
        let next_from_disk = scan_max_id(root) + 1;
        let next_from_seq = read_seq(root);
        let (tx, _) = broadcast::channel(64);
        Ok(Arc::new(SubmissionStore {
            root: root.to_path_buf(),
            next_id: AtomicU64::new(next_from_disk.max(next_from_seq)),
            tx,
            waiters: AtomicUsize::new(0),
            stream_waiters: AtomicUsize::new(0),
            stream_per_key: Mutex::new(HashMap::new()),
            rate: Mutex::new(HashMap::new()),
            record_locks: std::array::from_fn(|_| Mutex::new(())),
        }))
    }

    /// Lock the fixed stripe for `key`. Colliding keys may briefly serialize, but
    /// unrelated pages normally proceed independently and the lock map cannot grow.
    fn lock_records(&self, key: &str) -> MutexGuard<'_, ()> {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let shard = hasher.finish() as usize % RECORD_LOCK_SHARDS;
        self.record_locks[shard]
            .lock()
            .unwrap_or_else(|p| p.into_inner())
    }

    /// Subscribe to submit notifications (one receiver per held long-poll).
    fn subscribe(&self) -> broadcast::Receiver<Arc<str>> {
        self.tx.subscribe()
    }

    /// Check + record the rate limit for `key`. Returns `false` (reject) when the
    /// key has already had [`RATE_MAX`] accepted submits within [`RATE_WINDOW`], or
    /// when the tracking map is full of *other* active keys (fail-closed). On accept
    /// it records `now` so the window slides.
    fn rate_check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut map = self.rate.lock().unwrap_or_else(|p| p.into_inner());
        // Bound the map BEFORE inserting a brand-new key: once it is at the cap, drop
        // keys whose window has fully expired (cheap amortized GC), and if that still
        // does not free a slot, fail closed. This must run before the `entry(...)`
        // below — otherwise the current key would already be present and the cap
        // could never admit the sweep (the bug this replaces).
        if !map.contains_key(key) && map.len() >= RATE_MAX_KEYS {
            map.retain(|_, hits| hits.iter().any(|t| now.duration_since(*t) < RATE_WINDOW));
            if map.len() >= RATE_MAX_KEYS {
                return false;
            }
        }
        // Prune this key's expired hits, then admit iff under the per-key budget.
        let entry = map.entry(key.to_string()).or_default();
        while entry
            .front()
            .is_some_and(|t| now.duration_since(*t) >= RATE_WINDOW)
        {
            entry.pop_front();
        }
        if entry.len() >= RATE_MAX {
            return false;
        }
        entry.push_back(now);
        true
    }

    /// Persist one submission for `key`. `key`/`artifact`/`tenant`/`content_version`
    /// come from the trusted request context (the caller derived them, never the
    /// payload); `data` is the untrusted payload. Enforces the size cap, the
    /// rate limit, and the per-key total cap, then writes durably (fsync + atomic
    /// rename) **before** notifying waiters — so a returned submission is always on
    /// disk. `key` must be a valid space/slug name (path-safe component).
    pub fn submit(
        &self,
        key: &str,
        artifact: &str,
        tenant: &str,
        content_version: &str,
        data: serde_json::Value,
    ) -> Result<Submission, SubmitError> {
        // Size cap on the serialized payload (structural bound; never interpolated).
        let data_bytes = serde_json::to_vec(&data).map_err(|e| {
            SubmitError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        if data_bytes.len() > MAX_SUBMISSION_BYTES {
            return Err(SubmitError::TooLarge);
        }
        if !self.rate_check(key) {
            return Err(SubmitError::RateLimited);
        }

        let _records = self.lock_records(key);
        let key_dir = self.root.join(key);
        // Per-key total cap (best-effort count of existing records).
        if count_records(&key_dir) >= MAX_SUBMISSIONS_PER_KEY {
            return Err(SubmitError::Full);
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let record = Submission {
            schema: SUBMISSION_SCHEMA,
            id,
            key: key.to_string(),
            artifact: artifact.to_string(),
            tenant: tenant.to_string(),
            content_version: content_version.to_string(),
            created_at: Utc::now(),
            status_token: token::generate_token(),
            collected: false,
            data,
        };
        self.write_record(&key_dir, id, &record)
            .map_err(SubmitError::Io)?;
        // Notify held waiters only after the record is durable, carrying the key so a
        // waiter for a different key can ignore this wake. A send error just means no
        // one is waiting — the submission is on disk for the next poll.
        let _ = self.tx.send(Arc::from(key));
        Ok(record)
    }

    /// Durably materialize `<key>/<id>.json` (fsync + atomic rename), creating the
    /// key directory (and fsync'ing the root) on first use.
    fn write_record(&self, key_dir: &Path, id: u64, record: &Submission) -> std::io::Result<()> {
        let dir_is_new = !key_dir.exists();
        std::fs::create_dir_all(key_dir)?;
        let final_path = key_dir.join(format!("{id}.json"));
        let tmp_path = key_dir.join(format!(".{id}.tmp"));
        let json = serde_json::to_vec(record)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let staged = (|| -> std::io::Result<()> {
            write_file_synced(&tmp_path, &json)?;
            std::fs::rename(&tmp_path, &final_path)?;
            fsync_dir(key_dir)?;
            if dir_is_new {
                // Persist the new key-directory entry in the root before returning.
                fsync_dir(&self.root)?;
            }
            Ok(())
        })();
        if staged.is_err() {
            let _ = std::fs::remove_file(&tmp_path);
        }
        staged
    }

    /// Every submission for `key` with `id > since`, ordered by id, at most `max`.
    /// Returns the list and the new cursor (the last returned id, else `since`).
    /// Corrupt/oversize records are skipped with a log line, never fatal.
    ///
    /// Only the cheap `<id>.json` **filenames** are scanned + sorted first; then the
    /// at-most-`max` selected files are opened and parsed. A page with thousands of
    /// stored submissions therefore reads `max` files per poll, not the whole
    /// directory — the difference between an O(max) and an O(N) poll (and the `wait`
    /// long-poll re-runs this on every wake).
    pub fn list_since(&self, key: &str, since: u64, max: usize) -> std::io::Result<ListPage> {
        let _records = self.lock_records(key);
        let key_dir = self.root.join(key);
        let rd = match std::fs::read_dir(&key_dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ListPage {
                    submissions: Vec::new(),
                    cursor: since,
                });
            }
            Err(e) => return Err(e),
        };
        // Phase 1 — filenames only: collect the ids after the cursor (no file reads).
        let mut ids: Vec<u64> = Vec::new();
        for entry in rd.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // Only `<id>.json` records; skip `.<id>.tmp` staging + anything else.
            let Some(stem) = name.strip_suffix(".json") else {
                continue;
            };
            let Ok(id) = stem.parse::<u64>() else {
                continue;
            };
            if id > since {
                ids.push(id);
            }
        }
        // Phase 2 — sort ids, take the page, then read+parse only those files.
        ids.sort_unstable();
        ids.truncate(max);
        let mut out: Vec<Submission> = Vec::with_capacity(ids.len());
        for id in ids {
            let path = key_dir.join(format!("{id}.json"));
            match read_record(&path) {
                Ok(Some(mut rec)) if rec.id == id && rec.key == key => {
                    // Existing owner-scoped read/drain/wait/stream behavior IS the
                    // acknowledgement. Persist it before returning the record, so a
                    // page status poll never claims collection merely from memory.
                    if !rec.collected {
                        rec.collected = true;
                        if let Err(e) = self.write_record(&key_dir, id, &rec) {
                            // Collection feedback is secondary: never make an
                            // already-readable submission unavailable to the agent
                            // because its status rewrite failed. The disk record
                            // remains waiting, which is conservative and honest.
                            rec.collected = false;
                            eprintln!(
                                "glasspad: could not persist collection state for {}: {e}",
                                path.display()
                            );
                        }
                    }
                    out.push(rec);
                }
                Ok(_) => {}
                Err(e) => eprintln!(
                    "glasspad: skipping unreadable submission {}: {e}",
                    path.display()
                ),
            }
        }
        let cursor = out.last().map(|s| s.id).unwrap_or(since);
        Ok(ListPage {
            submissions: out,
            cursor,
        })
    }

    /// Read the delivery state for one exact `(key, tenant, id, status_token)` tuple.
    /// The token is a 128-bit random capability returned only to the submitting
    /// shell. Unknown tokens, tokens from another key, and tenant mismatches all
    /// collapse to `None`; callers must expose the same opaque 404 for every case.
    pub fn delivery_status(
        &self,
        key: &str,
        tenant: &str,
        id: u64,
        status_token: &str,
    ) -> std::io::Result<Option<DeliveryStatus>> {
        if !valid_space(key)
            || status_token.len() != 32
            || !status_token
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Ok(None);
        }
        // `id` is already returned by submit and is not a secret. Using it as the
        // record address makes this public polling path one bounded file read rather
        // than an attacker-amplifiable scan of up to 10,000 JSON records. Atomic
        // rename means no lock is needed: this sees the old/new record or NotFound.
        let path = self.root.join(key).join(format!("{id}.json"));
        let rec = match read_record(&path) {
            Ok(Some(rec)) => rec,
            Ok(None) => return Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        if rec.id == id
            && rec.key == key
            && rec.tenant == tenant
            && token::verify_token(status_token, &rec.status_token)
        {
            Ok(Some(if rec.collected {
                DeliveryStatus::Collected
            } else {
                DeliveryStatus::Waiting
            }))
        } else {
            Ok(None)
        }
    }

    /// Try to reserve one of the [`MAX_WAITERS`] long-poll slots. `None` when the
    /// cap is reached (the caller returns "too busy" rather than holding another
    /// connection). The returned guard releases the slot on drop.
    fn try_acquire_waiter(self: &Arc<Self>) -> Option<WaiterGuard> {
        let prev = self.waiters.fetch_add(1, Ordering::SeqCst);
        if prev >= MAX_WAITERS {
            self.waiters.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(WaiterGuard {
            store: self.clone(),
        })
    }

    /// Try to reserve one SSE-stream slot for `key`, enforcing **both** the global
    /// [`MAX_STREAM_WAITERS`] budget (separate from the long-poll budget, so streams
    /// never starve `wait`) **and** the per-key [`MAX_STREAMS_PER_KEY`] cap (so one
    /// page cannot monopolize the stream budget). `None` when either is reached. The
    /// per-key count is taken first under the lock and rolled back if the global slot
    /// is unavailable, so the two counters never drift. The returned guard releases
    /// both on drop.
    fn try_acquire_stream(self: &Arc<Self>, key: &str) -> Option<StreamGuard> {
        // Per-key admission first (under the lock).
        {
            let mut map = self
                .stream_per_key
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            let count = map.entry(key.to_string()).or_insert(0);
            if *count >= MAX_STREAMS_PER_KEY {
                if *count == 0 {
                    map.remove(key); // never leave a zero entry we just inserted
                }
                return None;
            }
            *count += 1;
        }
        // Global stream budget.
        let prev = self.stream_waiters.fetch_add(1, Ordering::SeqCst);
        if prev >= MAX_STREAM_WAITERS {
            self.stream_waiters.fetch_sub(1, Ordering::SeqCst);
            self.release_stream_key(key); // roll back the per-key increment
            return None;
        }
        Some(StreamGuard {
            store: self.clone(),
            key: key.to_string(),
        })
    }

    /// Decrement (and prune at zero) the per-key held-stream count. Paired with the
    /// increment in [`try_acquire_stream`]; called from [`StreamGuard::drop`].
    fn release_stream_key(&self, key: &str) {
        let mut map = self
            .stream_per_key
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if let Some(count) = map.get_mut(key) {
            *count -= 1;
            if *count == 0 {
                map.remove(key);
            }
        }
    }

    /// Remove submissions older than `retention`, then reap emptied key
    /// directories and leftover `.tmp` staging files. Returns the number removed.
    /// Best-effort per-file; a single failure is logged, not fatal.
    pub fn gc(&self, retention: ChronoDuration) -> std::io::Result<usize> {
        let cutoff = glasspad::time::retention_cutoff(&crate::clock::SystemClock, retention);
        let mut removed = 0usize;
        let rd = match std::fs::read_dir(&self.root) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e),
        };
        for entry in rd.flatten() {
            let key_dir = entry.path();
            if !key_dir.is_dir() {
                continue;
            }
            let key = entry.file_name().to_string_lossy().into_owned();
            let _records = self.lock_records(&key);
            let files = match std::fs::read_dir(&key_dir) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let mut dirty = false;
            let mut remaining = 0usize;
            for file in files.flatten() {
                let path = file.path();
                let name = file.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.') {
                    // Reap a leftover staging file from a crashed write.
                    let _ = std::fs::remove_file(&path);
                    dirty = true;
                    continue;
                }
                let expired = match read_record(&path) {
                    Ok(Some(rec)) => rec.created_at < cutoff,
                    // Corrupt/unreadable record: drop it (it can never be delivered).
                    _ => true,
                };
                if expired {
                    if std::fs::remove_file(&path).is_ok() {
                        removed += 1;
                        dirty = true;
                    }
                } else {
                    remaining += 1;
                }
            }
            if dirty {
                let _ = fsync_dir(&key_dir);
            }
            // Reap a now-empty key directory so the tree stays bounded to live keys.
            if remaining == 0 {
                let _ = std::fs::remove_dir(&key_dir);
            }
        }
        // Persist the id high-water mark so a store whose records were all reaped
        // still recovers a monotonic counter on the next open — otherwise `next_id`
        // would reset to 1 and a long-lived cursor would skip every new submission.
        let _ = write_seq(&self.root, self.next_id.load(Ordering::SeqCst));
        Ok(removed)
    }
}

/// The only two states exposed by the page-reachable status read. `Waiting` means
/// durably stored but not yet selected by an owner-scoped agent read; it is not a
/// storage failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryStatus {
    Waiting,
    Collected,
}

/// One page of submissions plus the cursor to poll from next.
pub struct ListPage {
    pub submissions: Vec<Submission>,
    pub cursor: u64,
}

/// The outcome of a long-poll [`wait`].
pub enum WaitOutcome {
    /// At least one submission landed after the cursor.
    Ready(ListPage),
    /// The timeout fired with nothing new (the cursor is unchanged). A *distinct*
    /// result so a backgrounded agent can tell "no answer yet" from "here it is".
    TimedOut { cursor: u64 },
    /// The store is already holding [`MAX_WAITERS`] long-polls; the caller should
    /// fall back to plain polling.
    TooBusy,
}

/// Server-side long-poll: hold until a submission for `key` after `since` lands or
/// `timeout` elapses, whichever first. Bounded by [`MAX_WAITERS`] concurrent holders
/// and by `timeout` (already clamped by the caller). The subscribe-before-check
/// ordering guarantees no lost wakeup: a submit that notifies after we subscribe is
/// received; one that landed before is seen by the initial `list_since`. Each notify
/// carries its `key`, so a wake for a *different* key is ignored WITHOUT re-running
/// `list_since` (no store-wide thundering herd); `Lagged` (a burst overran the
/// channel — we may have missed our own key) conservatively triggers a re-check.
pub async fn wait(
    store: Arc<SubmissionStore>,
    key: String,
    since: u64,
    timeout: Duration,
    max: usize,
) -> std::io::Result<WaitOutcome> {
    let Some(_guard) = store.try_acquire_waiter() else {
        return Ok(WaitOutcome::TooBusy);
    };
    // Subscribe BEFORE the first check so no notification is lost in the gap.
    let mut rx = store.subscribe();
    // One absolute deadline for the whole hold, pinned so re-entering the inner
    // select on an unrelated wake does NOT reset the timer (a fresh `sleep(remaining)`
    // each iteration would let unrelated traffic extend the hold indefinitely).
    let sleep = tokio::time::sleep(timeout);
    tokio::pin!(sleep);
    let want: Arc<str> = Arc::from(key.as_str());
    loop {
        let page = {
            let store = store.clone();
            let key = key.clone();
            tokio::task::spawn_blocking(move || store.list_since(&key, since, max))
                .await
                .map_err(|e| std::io::Error::other(format!("wait task panicked: {e}")))??
        };
        if !page.submissions.is_empty() {
            return Ok(WaitOutcome::Ready(page));
        }
        // Wait for a notification for OUR key (or the deadline). Unrelated-key wakes
        // loop here cheaply without touching the filesystem.
        loop {
            tokio::select! {
                _ = &mut sleep => return Ok(WaitOutcome::TimedOut { cursor: since }),
                r = rx.recv() => match r {
                    // Our key changed — break out to re-check via list_since.
                    Ok(k) if k == want => break,
                    // A different key — keep waiting, no re-check.
                    Ok(_) => continue,
                    // We may have missed our own key under a burst — re-check.
                    Err(broadcast::error::RecvError::Lagged(_)) => break,
                    // The sender was dropped (store gone): stop holding.
                    Err(broadcast::error::RecvError::Closed) => {
                        return Ok(WaitOutcome::TimedOut { cursor: since });
                    }
                },
            }
        }
    }
}

/// Buffered submissions in a stream channel before the pump applies backpressure.
/// Small on purpose: a fast producer parks the pump (never the store) once the
/// consumer falls this far behind, and a slow/dead consumer is detected within one
/// buffer rather than being allowed to queue unboundedly.
const STREAM_CHANNEL_CAP: usize = 64;

/// Open a **server-push stream** (A2) over the persisted-cursor store: yields each
/// submission for `key` with `id > since` as it lands, in id order, preserving the
/// exact no-redeliver / no-skip semantics of [`list_since`] and [`wait`]. Reuses the
/// same keyed broadcast (a wake for a different key is ignored without a filesystem
/// read). Held streams have their **own** budget ([`MAX_STREAM_WAITERS`] global +
/// [`MAX_STREAMS_PER_KEY`] per key), *separate* from the long-poll [`MAX_WAITERS`]
/// budget — so a flood of streams can never starve `await-submission`, and one page
/// cannot monopolize the stream budget. Returns `None` when either stream cap is
/// reached; the caller answers "too busy" and the agent falls back to polling.
///
/// The returned receiver streams until the consumer drops it (the SSE client
/// disconnects — axum's keep-alive turns a dead socket into a failed write, which
/// drops the response body and thus this receiver): the pump then observes the closed
/// channel and exits, releasing the stream slot. There is deliberately **no** server
/// hard timeout (unlike `wait`): a held stream is the point of A2 (watch many pages /
/// sub-second streaming), and the separate stream cap + disconnect detection bound it.
pub fn open_stream(
    store: Arc<SubmissionStore>,
    key: String,
    since: u64,
) -> Option<mpsc::Receiver<Submission>> {
    let guard = store.try_acquire_stream(&key)?;
    let (tx, rx) = mpsc::channel(STREAM_CHANNEL_CAP);
    tokio::spawn(stream_pump(store, key, since, tx, guard));
    Some(rx)
}

/// The push loop behind [`open_stream`]. Holds the stream slot (`_guard`) for its
/// whole lifetime and exits — freeing the slot — as soon as the consumer drops the
/// receiver. The subscribe-before-first-read ordering is the same lost-wakeup defense
/// as [`wait`]: a submit that notifies after we subscribe is received; one that landed
/// before is seen by the initial drain.
async fn stream_pump(
    store: Arc<SubmissionStore>,
    key: String,
    mut since: u64,
    tx: mpsc::Sender<Submission>,
    _guard: StreamGuard,
) {
    // Subscribe BEFORE the first drain so no notification is lost in the gap.
    let mut rx = store.subscribe();
    let want: Arc<str> = Arc::from(key.as_str());
    loop {
        // Drain everything after the cursor, re-reading until a page comes back EMPTY.
        // Draining until empty (rather than "stop when a page is not full") is correct
        // even when a page contains skipped/corrupt records: `list_since` advances the
        // cursor to the last GOOD id, so a mixed page returns fewer than MAX_LIST rows
        // yet more valid rows may lie beyond it — a "not full ⇒ done" test would leave
        // them undelivered until the next wake. `id > since` keeps every re-read
        // strictly forward, so this never re-delivers.
        loop {
            let page = {
                let store = store.clone();
                let key = key.clone();
                match tokio::task::spawn_blocking(move || store.list_since(&key, since, MAX_LIST))
                    .await
                {
                    Ok(Ok(page)) => page,
                    Ok(Err(e)) => {
                        eprintln!("glasspad: submission stream read error: {e}");
                        return;
                    }
                    Err(e) => {
                        eprintln!("glasspad: submission stream task panicked: {e}");
                        return;
                    }
                }
            };
            if page.submissions.is_empty() {
                break;
            }
            for sub in page.submissions {
                since = sub.id;
                if tx.send(sub).await.is_err() {
                    return; // consumer (SSE client) gone
                }
            }
        }
        // Park until OUR key changes or the consumer disconnects. Unrelated-key wakes
        // loop here cheaply without touching the filesystem (no thundering herd).
        loop {
            tokio::select! {
                _ = tx.closed() => return,
                r = rx.recv() => match r {
                    Ok(k) if k == want => break,
                    Ok(_) => continue,
                    // A burst overran the channel — we may have missed our key; re-check.
                    Err(broadcast::error::RecvError::Lagged(_)) => break,
                    // The store's sender was dropped: stop holding.
                    Err(broadcast::error::RecvError::Closed) => return,
                },
            }
        }
    }
}

/// Wrap a stream receiver from [`open_stream`] into an SSE response, so the hosted
/// and loopback handlers push **identical** frames (parity): each submission is a
/// `submission` event carrying its `to_public_json()` body, with the submission id
/// stamped as the SSE `id` — a valid per-key cursor now the stream is key-scoped, so
/// a browser `EventSource` resumes via `Last-Event-ID`. axum's keep-alive turns a
/// dead socket into a failed write, which drops the response body (and thus the pump's
/// receiver), so a disconnected client's waiter slot is reclaimed. The public JSON
/// never includes the internal `tenant` (see [`Submission::to_public_json`]).
pub fn submission_sse(
    rx: mpsc::Receiver<Submission>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = ReceiverStream::new(rx).map(|sub| {
        let id = sub.id;
        let event = Event::default()
            .event("submission")
            .id(id.to_string())
            .json_data(sub.to_public_json())
            // A `Value` of standard types always serializes; if it somehow does not,
            // still stamp the real id so the client can advance its cursor past this
            // record (an id-less event would wedge a client that keys off the id).
            .unwrap_or_else(|e| {
                eprintln!("glasspad: submission {id} failed to serialize for SSE: {e}");
                Event::default()
                    .event("submission")
                    .id(id.to_string())
                    .data(format!("{{\"id\":{id}}}"))
            });
        Ok::<Event, Infallible>(event)
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Parse a `Last-Event-ID` HTTP header value into a cursor. A reconnecting browser
/// `EventSource` re-sends the last delivered submission id here; a missing/unparseable
/// value yields `None` so the caller starts from its query default. Shared by the
/// hosted and loopback stream handlers so the cursor grammar can never drift between
/// them. It only ever selects *which already-persisted* records the (already key- and
/// tenant-scoped) caller re-reads — never a cross-key/-tenant escape.
pub fn parse_last_event_id(raw: Option<&str>) -> Option<u64> {
    raw.and_then(|s| s.trim().parse::<u64>().ok())
}

/// RAII release of a long-poll waiter slot.
struct WaiterGuard {
    store: Arc<SubmissionStore>,
}

impl Drop for WaiterGuard {
    fn drop(&mut self) {
        self.store.waiters.fetch_sub(1, Ordering::SeqCst);
    }
}

/// RAII release of an SSE-stream slot: drops both the global stream counter and the
/// per-key count (see [`SubmissionStore::try_acquire_stream`]). Held for the pump's
/// whole lifetime, so a returned/aborted/panicked pump reclaims both exactly once.
struct StreamGuard {
    store: Arc<SubmissionStore>,
    key: String,
}

impl Drop for StreamGuard {
    fn drop(&mut self) {
        self.store.stream_waiters.fetch_sub(1, Ordering::SeqCst);
        self.store.release_stream_key(&self.key);
    }
}

/// Count the `<id>.json` records under a key directory (ignoring staging files).
/// A missing directory is zero.
fn count_records(key_dir: &Path) -> usize {
    match std::fs::read_dir(key_dir) {
        Ok(rd) => rd
            .flatten()
            .filter(|e| {
                let n = e.file_name();
                let n = n.to_string_lossy();
                !n.starts_with('.') && n.ends_with(".json")
            })
            .count(),
        Err(_) => 0,
    }
}

/// The persisted "next id" high-water mark file (in the store root).
const SEQ_FILE: &str = ".seq";

/// Read the persisted "next id" high-water mark (0 if absent/unreadable). The caller
/// takes the max with the on-disk scan, so a missing/corrupt file can only lose the
/// GC-survives-empty guarantee — it can never cause id **reuse** (which would need a
/// value *below* an existing on-disk id, and the max() defends that).
fn read_seq(root: &Path) -> u64 {
    std::fs::read_to_string(root.join(SEQ_FILE))
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0)
}

/// Persist the "next id" high-water mark durably (atomic rename + fsync). It lives in
/// the store root as a `.`-prefixed file, so `scan_max_id`/`gc` (which only descend
/// into key directories) never mistake it for a record.
fn write_seq(root: &Path, next_id: u64) -> std::io::Result<()> {
    let final_path = root.join(SEQ_FILE);
    let tmp_path = root.join(".seq.tmp");
    write_file_synced(&tmp_path, next_id.to_string().as_bytes())?;
    std::fs::rename(&tmp_path, &final_path)?;
    fsync_dir(root)
}

/// Recover the highest submission id present anywhere under `root` (0 if none), so
/// the in-memory counter resumes above every persisted id after a restart.
fn scan_max_id(root: &Path) -> u64 {
    let mut max = 0u64;
    let Ok(keys) = std::fs::read_dir(root) else {
        return 0;
    };
    for key in keys.flatten() {
        if !key.path().is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(key.path()) else {
            continue;
        };
        for file in files.flatten() {
            let name = file.file_name();
            let name = name.to_string_lossy();
            if let Some(stem) = name.strip_suffix(".json")
                && let Ok(id) = stem.parse::<u64>()
            {
                max = max.max(id);
            }
        }
    }
    max
}

/// Read + parse one record file, bounded. Returns `None` for an oversize/invalid
/// record (skipped by the caller) rather than erroring, so one bad file never
/// blocks a whole poll.
fn read_record(path: &Path) -> std::io::Result<Option<Submission>> {
    let f = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    f.take(MAX_RECORD_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Ok(None);
    }
    match serde_json::from_slice::<Submission>(&bytes) {
        Ok(rec) if rec.schema == SUBMISSION_SCHEMA && valid_space(&rec.key) => Ok(Some(rec)),
        _ => Ok(None),
    }
}

/// Write `bytes` to `path` and fsync it so its contents are durable before the
/// caller renames it into place (create truncates any stale tmp file).
fn write_file_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut f = std::fs::File::create(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}

/// fsync a directory so a rename/create within it is durable (Unix). A no-op where
/// a directory handle can't be fsync'd; the deploy target is Unix.
#[cfg(unix)]
fn fsync_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::File::open(dir)?.sync_all()
}

#[cfg(not(unix))]
fn fsync_dir(_dir: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "gp-subs-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Write `n` records (ids `1..=n`) for `key` straight to disk, bypassing `submit`'s
    /// per-key rate limit so a backlog larger than `MAX_LIST` can be exercised.
    fn seed_records(store: &SubmissionStore, key: &str, n: u64) {
        let key_dir = store.root.join(key);
        for id in 1..=n {
            let rec = Submission {
                schema: SUBMISSION_SCHEMA,
                id,
                key: key.to_string(),
                artifact: "index".into(),
                tenant: "acme".into(),
                content_version: "v1".into(),
                created_at: Utc::now(),
                status_token: token::generate_token(),
                collected: false,
                data: serde_json::json!({ "n": id }),
            };
            store.write_record(&key_dir, id, &rec).unwrap();
        }
    }

    #[test]
    fn exact_delivery_status_changes_only_after_agent_collection() {
        let root = tmp_root("delivery-status");
        let store = SubmissionStore::open(&root).unwrap();
        let sub = store
            .submit("abc", "index", "acme", "v1", serde_json::json!({"a": 1}))
            .unwrap();
        let receipt = sub.status_token().to_string();

        assert_eq!(
            store
                .delivery_status("abc", "acme", sub.id, &receipt)
                .unwrap(),
            Some(DeliveryStatus::Waiting)
        );
        assert_eq!(
            store
                .delivery_status("other", "acme", sub.id, &receipt)
                .unwrap(),
            None
        );
        assert_eq!(
            store
                .delivery_status("abc", "globex", sub.id, &receipt)
                .unwrap(),
            None
        );
        assert_eq!(
            store
                .delivery_status("abc", "acme", sub.id, "00000000000000000000000000000000",)
                .unwrap(),
            None
        );

        let page = store.list_since("abc", 0, MAX_LIST).unwrap();
        assert_eq!(page.submissions.len(), 1);
        assert_eq!(
            store
                .delivery_status("abc", "acme", sub.id, &receipt)
                .unwrap(),
            Some(DeliveryStatus::Collected)
        );
        drop(store);

        // Both the opaque capability and collection state survive a restart.
        let reopened = SubmissionStore::open(&root).unwrap();
        assert_eq!(
            reopened
                .delivery_status("abc", "acme", sub.id, &receipt)
                .unwrap(),
            Some(DeliveryStatus::Collected)
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn bounded_read_collects_only_the_returned_submission() {
        let root = tmp_root("status-bounded");
        let store = SubmissionStore::open(&root).unwrap();
        let first = store
            .submit("abc", "index", "acme", "v1", serde_json::json!({"n": 1}))
            .unwrap();
        let second = store
            .submit("abc", "index", "acme", "v1", serde_json::json!({"n": 2}))
            .unwrap();

        let page = store.list_since("abc", 0, 1).unwrap();
        assert_eq!(page.submissions.len(), 1);
        assert_eq!(page.submissions[0].id, first.id);
        assert_eq!(
            store
                .delivery_status("abc", "acme", first.id, first.status_token())
                .unwrap(),
            Some(DeliveryStatus::Collected)
        );
        assert_eq!(
            store
                .delivery_status("abc", "acme", second.id, second.status_token())
                .unwrap(),
            Some(DeliveryStatus::Waiting)
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn collection_state_write_failure_never_breaks_agent_read() {
        use std::os::unix::fs::PermissionsExt;

        let root = tmp_root("status-write-failure");
        let store = SubmissionStore::open(&root).unwrap();
        let sub = store
            .submit(
                "abc",
                "index",
                "acme",
                "v1",
                serde_json::json!({"answer": 1}),
            )
            .unwrap();
        let key_dir = root.join("abc");
        std::fs::set_permissions(&key_dir, std::fs::Permissions::from_mode(0o500)).unwrap();

        let page = store.list_since("abc", 0, MAX_LIST).unwrap();
        assert_eq!(
            page.submissions.len(),
            1,
            "the primary read must still work"
        );

        std::fs::set_permissions(&key_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            store
                .delivery_status("abc", "acme", sub.id, sub.status_token())
                .unwrap(),
            Some(DeliveryStatus::Waiting),
            "a failed marker write must conservatively remain waiting"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn status_capability_is_not_exposed_to_agent_reads() {
        let root = tmp_root("status-private");
        let store = SubmissionStore::open(&root).unwrap();
        let sub = store
            .submit("abc", "index", "acme", "v1", serde_json::json!({}))
            .unwrap();
        let public = sub.to_public_json();
        assert!(public.get("status_token").is_none());
        assert!(public.get("collected").is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn submit_persists_and_lists_since_cursor() {
        let root = tmp_root("list");
        let store = SubmissionStore::open(&root).unwrap();
        let s1 = store
            .submit("abc", "index", "acme", "v1", serde_json::json!({"a": 1}))
            .unwrap();
        let s2 = store
            .submit("abc", "index", "acme", "v1", serde_json::json!({"a": 2}))
            .unwrap();
        assert!(s2.id > s1.id, "ids must be monotonic");

        // From cursor 0: both, ordered.
        let page = store.list_since("abc", 0, MAX_LIST).unwrap();
        assert_eq!(page.submissions.len(), 2);
        assert_eq!(page.submissions[0].id, s1.id);
        assert_eq!(page.cursor, s2.id);

        // From the first id: only the second.
        let page = store.list_since("abc", s1.id, MAX_LIST).unwrap();
        assert_eq!(page.submissions.len(), 1);
        assert_eq!(page.submissions[0].id, s2.id);

        // A different key sees nothing.
        assert!(
            store
                .list_since("other", 0, MAX_LIST)
                .unwrap()
                .submissions
                .is_empty()
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn ids_survive_reopen_monotonic() {
        let root = tmp_root("reopen");
        let store = SubmissionStore::open(&root).unwrap();
        let s1 = store
            .submit("abc", "index", "acme", "v1", serde_json::json!({}))
            .unwrap();
        drop(store);
        // A fresh store recovers the counter above the max on disk.
        let store2 = SubmissionStore::open(&root).unwrap();
        let s2 = store2
            .submit("abc", "index", "acme", "v1", serde_json::json!({}))
            .unwrap();
        assert!(s2.id > s1.id, "id counter must resume above on-disk max");
        // Both are still listed (the first survived the reopen).
        assert_eq!(
            store2
                .list_since("abc", 0, MAX_LIST)
                .unwrap()
                .submissions
                .len(),
            2
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn oversize_payload_rejected() {
        let root = tmp_root("big");
        let store = SubmissionStore::open(&root).unwrap();
        let big = "x".repeat(MAX_SUBMISSION_BYTES + 1);
        let err = store
            .submit(
                "abc",
                "index",
                "acme",
                "v1",
                serde_json::json!({ "f": big }),
            )
            .unwrap_err();
        assert!(matches!(err, SubmitError::TooLarge));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rate_limit_rejects_a_flood() {
        let root = tmp_root("rate");
        let store = SubmissionStore::open(&root).unwrap();
        let mut ok = 0;
        let mut limited = 0;
        for _ in 0..(RATE_MAX + 10) {
            match store.submit("abc", "index", "acme", "v1", serde_json::json!({})) {
                Ok(_) => ok += 1,
                Err(SubmitError::RateLimited) => limited += 1,
                Err(e) => panic!("unexpected {e}"),
            }
        }
        assert_eq!(ok, RATE_MAX, "exactly the window budget is accepted");
        assert!(limited >= 10, "the rest are rate-limited");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn gc_removes_expired_and_reaps_empty_dirs() {
        let root = tmp_root("gc");
        let store = SubmissionStore::open(&root).unwrap();
        store
            .submit("abc", "index", "acme", "v1", serde_json::json!({}))
            .unwrap();
        // Backdate the on-disk record.
        let page = store.list_since("abc", 0, MAX_LIST).unwrap();
        let path = root
            .join("abc")
            .join(format!("{}.json", page.submissions[0].id));
        let mut rec: Submission = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        rec.created_at = Utc::now() - ChronoDuration::days(10);
        std::fs::write(&path, serde_json::to_vec(&rec).unwrap()).unwrap();

        let removed = store.gc(ChronoDuration::days(7)).unwrap();
        assert_eq!(removed, 1);
        assert!(!root.join("abc").exists(), "emptied key dir is reaped");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn id_counter_survives_a_fully_gc_emptied_store() {
        // Regression: after GC reaps every record, a reopened store must NOT reset its
        // id to 1 — otherwise an agent polling with an old cursor (since=N) would
        // silently skip every new submission. The persisted high-water mark keeps ids
        // monotonic across a full reap.
        let root = tmp_root("idseq");
        let store = SubmissionStore::open(&root).unwrap();
        // Advance the counter with a few submissions.
        let mut last = 0;
        for _ in 0..5 {
            last = store
                .submit("abc", "index", "acme", "v1", serde_json::json!({}))
                .unwrap()
                .id;
        }
        // Backdate + GC them all so the store is emptied of records.
        for f in std::fs::read_dir(root.join("abc")).unwrap().flatten() {
            let p = f.path();
            let mut rec: Submission = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
            rec.created_at = Utc::now() - ChronoDuration::days(30);
            std::fs::write(&p, serde_json::to_vec(&rec).unwrap()).unwrap();
        }
        assert_eq!(store.gc(ChronoDuration::days(7)).unwrap(), 5);
        assert!(!root.join("abc").exists(), "records fully reaped");
        drop(store);

        // Reopen: the next id must still be ABOVE every previously-allocated id.
        let store2 = SubmissionStore::open(&root).unwrap();
        let next = store2
            .submit("abc", "index", "acme", "v1", serde_json::json!({}))
            .unwrap();
        assert!(
            next.id > last,
            "id reset after full GC: {} !> {last}",
            next.id
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn wait_ignores_submissions_for_other_keys() {
        // Keyed broadcast: a submit to a DIFFERENT key must not satisfy a waiter — it
        // holds until its own key lands or the timeout fires.
        let root = tmp_root("waitkey");
        let store = SubmissionStore::open(&root).unwrap();
        let s = store.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(40)).await;
            // Noise on an unrelated key…
            s.submit("other", "index", "acme", "v1", serde_json::json!({}))
                .unwrap();
        });
        // Waiting on "abc" with only an "other" submit → times out (not satisfied).
        let outcome = wait(store, "abc".into(), 0, Duration::from_millis(150), MAX_LIST)
            .await
            .unwrap();
        assert!(
            matches!(outcome, WaitOutcome::TimedOut { .. }),
            "an unrelated-key submit must not satisfy a wait"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn content_version_is_stable_and_body_sensitive() {
        assert_eq!(content_version("hello"), content_version("hello"));
        assert_ne!(content_version("hello"), content_version("hell0"));
        assert_eq!(content_version("hello").len(), 16);
    }

    #[tokio::test]
    async fn wait_returns_a_submission_that_lands_during_the_hold() {
        let root = tmp_root("wait");
        let store = SubmissionStore::open(&root).unwrap();
        let s = store.clone();
        // Land a submission shortly after the wait begins.
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            s.submit(
                "abc",
                "index",
                "acme",
                "v1",
                serde_json::json!({"ok": true}),
            )
            .unwrap();
        });
        let outcome = wait(
            store.clone(),
            "abc".into(),
            0,
            Duration::from_secs(5),
            MAX_LIST,
        )
        .await
        .unwrap();
        match outcome {
            WaitOutcome::Ready(page) => {
                assert_eq!(page.submissions.len(), 1);
                assert_eq!(page.submissions[0].data["ok"], serde_json::json!(true));
            }
            _ => panic!("expected a ready submission"),
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn wait_times_out_distinctly_when_nothing_lands() {
        let root = tmp_root("waitto");
        let store = SubmissionStore::open(&root).unwrap();
        let outcome = wait(store, "abc".into(), 0, Duration::from_millis(80), MAX_LIST)
            .await
            .unwrap();
        assert!(
            matches!(outcome, WaitOutcome::TimedOut { cursor: 0 }),
            "a hold with no submission must time out distinctly"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn wait_returns_already_pending_without_holding() {
        let root = tmp_root("waitpending");
        let store = SubmissionStore::open(&root).unwrap();
        let pending = store
            .submit("abc", "index", "acme", "v1", serde_json::json!({"n": 1}))
            .unwrap();
        // A submission that landed BEFORE the wait (between arm calls) is returned
        // on the next `since`, never missed.
        let outcome = wait(
            store.clone(),
            "abc".into(),
            0,
            Duration::from_secs(5),
            MAX_LIST,
        )
        .await
        .unwrap();
        assert!(matches!(outcome, WaitOutcome::Ready(_)));
        assert_eq!(
            store
                .delivery_status("abc", "acme", pending.id, pending.status_token())
                .unwrap(),
            Some(DeliveryStatus::Collected),
            "the existing long-poll path is an acknowledgement"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn stream_backfills_then_pushes_live_in_order() {
        // A2: the stream first drains everything after the cursor (backfill), then
        // pushes each further submission live, all in id order and none skipped.
        let root = tmp_root("stream");
        let store = SubmissionStore::open(&root).unwrap();
        let a = store
            .submit("abc", "index", "acme", "v1", serde_json::json!({"n": 1}))
            .unwrap();
        let b = store
            .submit("abc", "index", "acme", "v1", serde_json::json!({"n": 2}))
            .unwrap();
        let mut rx = open_stream(store.clone(), "abc".into(), 0).expect("a slot is free");
        // Backfill: the two already-persisted submissions arrive first, in order.
        let first = rx.recv().await.unwrap();
        let second = rx.recv().await.unwrap();
        assert_eq!(first.id, a.id);
        assert_eq!(second.id, b.id);
        // A submission landing DURING the hold is pushed live.
        let c = store
            .submit("abc", "index", "acme", "v1", serde_json::json!({"n": 3}))
            .unwrap();
        let third = rx.recv().await.unwrap();
        assert_eq!(third.id, c.id);
        assert!(third.id > second.id, "ids stream monotonically");
        for sub in [&a, &b, &c] {
            assert_eq!(
                store
                    .delivery_status("abc", "acme", sub.id, sub.status_token())
                    .unwrap(),
                Some(DeliveryStatus::Collected),
                "the existing SSE stream path is an acknowledgement"
            );
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn stream_honors_the_cursor_no_redeliver() {
        // A reconnect resumes from `since`: a submission at/under the cursor is never
        // re-delivered, only strictly-newer ones stream.
        let root = tmp_root("streamcur");
        let store = SubmissionStore::open(&root).unwrap();
        let seen = store
            .submit(
                "abc",
                "index",
                "acme",
                "v1",
                serde_json::json!({"old": true}),
            )
            .unwrap();
        let mut rx = open_stream(store.clone(), "abc".into(), seen.id).expect("a slot is free");
        // Nothing newer yet → the stream must not hand back the already-seen record.
        assert!(
            tokio::time::timeout(Duration::from_millis(120), rx.recv())
                .await
                .is_err(),
            "a cursor at the last id must re-deliver nothing"
        );
        // Only a strictly-newer submission streams.
        let fresh = store
            .submit(
                "abc",
                "index",
                "acme",
                "v1",
                serde_json::json!({"new": true}),
            )
            .unwrap();
        let got = rx.recv().await.unwrap();
        assert_eq!(got.id, fresh.id);
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn stream_ignores_other_keys() {
        // Per-tenant/-space isolation at the store: a submit to a DIFFERENT key never
        // appears on a stream bound to `abc`.
        let root = tmp_root("streamkey");
        let store = SubmissionStore::open(&root).unwrap();
        let mut rx = open_stream(store.clone(), "abc".into(), 0).expect("a slot is free");
        store
            .submit("other", "index", "acme", "v1", serde_json::json!({}))
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(120), rx.recv())
                .await
                .is_err(),
            "an unrelated-key submit must not appear on this stream"
        );
        // The stream's OWN key still delivers.
        let mine = store
            .submit("abc", "index", "acme", "v1", serde_json::json!({}))
            .unwrap();
        assert_eq!(rx.recv().await.unwrap().id, mine.id);
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn stream_too_busy_at_the_global_stream_cap_without_starving_waits() {
        // Streams have their OWN budget, separate from the long-poll budget: past the
        // global stream cap `open_stream` returns None, but a long-poll `wait` is NOT
        // refused (streams can never exhaust the primary long-poll surface).
        let root = tmp_root("streambusy");
        let store = SubmissionStore::open(&root).unwrap();
        // Fill the global stream budget with DISTINCT keys (so the per-key cap is not
        // what rejects — this exercises the global `MAX_STREAM_WAITERS` bound).
        let mut held = Vec::new();
        for i in 0..MAX_STREAM_WAITERS {
            held.push(
                open_stream(store.clone(), format!("k{i}"), 0).expect("under the global cap"),
            );
        }
        assert!(
            open_stream(store.clone(), "knew".into(), 0).is_none(),
            "a stream is refused once the global stream cap is reached"
        );
        // The long-poll budget is untouched — a wait still holds (times out, not busy).
        let outcome = wait(
            store.clone(),
            "kwait".into(),
            0,
            Duration::from_millis(50),
            MAX_LIST,
        )
        .await
        .unwrap();
        assert!(
            matches!(outcome, WaitOutcome::TimedOut { .. }),
            "streams at their cap must not exhaust the separate long-poll budget"
        );
        // Dropping one held stream frees a global slot (the pump exits, releasing it).
        held.pop();
        for _ in 0..50 {
            if store.stream_waiters.load(Ordering::SeqCst) < MAX_STREAM_WAITERS {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            open_stream(store.clone(), "kafter".into(), 0).is_some(),
            "a global stream slot frees up after a held stream is dropped"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn stream_per_key_cap_bounds_one_key_only() {
        // One page/space cannot monopolize the stream budget: past MAX_STREAMS_PER_KEY
        // for a key, that key is refused while a DIFFERENT key is still admitted.
        let root = tmp_root("streamperkey");
        let store = SubmissionStore::open(&root).unwrap();
        let mut held = Vec::new();
        for _ in 0..MAX_STREAMS_PER_KEY {
            held.push(open_stream(store.clone(), "abc".into(), 0).expect("under the per-key cap"));
        }
        assert!(
            open_stream(store.clone(), "abc".into(), 0).is_none(),
            "one key cannot exceed the per-key stream cap"
        );
        assert!(
            open_stream(store.clone(), "xyz".into(), 0).is_some(),
            "the per-key cap does not block a different key"
        );
        // Dropping one `abc` stream frees a per-key slot.
        held.pop();
        for _ in 0..50 {
            let n = store
                .stream_per_key
                .lock()
                .unwrap()
                .get("abc")
                .copied()
                .unwrap_or(0);
            if n < MAX_STREAMS_PER_KEY {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            open_stream(store.clone(), "abc".into(), 0).is_some(),
            "a per-key slot frees up after a held stream is dropped"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn stream_delivers_a_backlog_larger_than_one_page() {
        // A backlog spanning more than one `list_since` page (MAX_LIST) is streamed in
        // full, in id order, with neither skip nor duplicate — the drain-until-empty
        // contract, not "stop when a page is not full".
        let root = tmp_root("streambacklog");
        let store = SubmissionStore::open(&root).unwrap();
        let total = MAX_LIST as u64 + 25;
        seed_records(&store, "abc", total);
        let mut rx = open_stream(store.clone(), "abc".into(), 0).expect("a slot is free");
        for expect in 1..=total {
            let got = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .expect("a record arrives")
                .expect("channel open");
            assert_eq!(
                got.id, expect,
                "records stream in id order across page boundaries"
            );
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn stream_drains_past_a_corrupt_record_in_a_full_page() {
        // Regression for the `full = len >= MAX_LIST` under-drain bug: a corrupt record
        // inside the first page made the page look "not full", so records BEYOND it were
        // withheld until the next wake. Draining until an empty page delivers them now.
        let root = tmp_root("streamcorrupt");
        let store = SubmissionStore::open(&root).unwrap();
        let total = MAX_LIST as u64 + 1; // id `total` lies beyond the first 100-id page
        seed_records(&store, "abc", total);
        // Corrupt id 50 (inside the first page) so it is skipped by `list_since`.
        std::fs::write(root.join("abc").join("50.json"), b"{ not json").unwrap();
        let mut rx = open_stream(store.clone(), "abc".into(), 0).expect("a slot is free");
        let mut seen = Vec::new();
        for _ in 0..(total - 1) {
            let got = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .expect("a record arrives")
                .expect("channel open");
            seen.push(got.id);
        }
        assert!(!seen.contains(&50), "the corrupt record is skipped");
        assert!(
            seen.contains(&total),
            "id {total} beyond the corrupt page is still delivered"
        );
        assert_eq!(
            seen.len(),
            (total - 1) as usize,
            "every good record arrives exactly once"
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
