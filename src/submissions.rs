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
//! waiter cap so held connections can never grow without limit.

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;

use crate::artifact_host::valid_space;

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

/// Sliding-window rate limit on **accepted submits** per key.
const RATE_MAX: usize = 30;
const RATE_WINDOW: Duration = Duration::from_secs(60);
/// Cap on the number of distinct keys the rate-limiter tracks, so a flood of
/// one-off keys can't grow the map without bound (a full map rejects new keys
/// until the sweep below prunes emptied entries — fail-closed on the rate path).
const RATE_MAX_KEYS: usize = 4096;

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
    /// The untrusted user payload.
    pub data: serde_json::Value,
}

impl Submission {
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
    /// Fired after each durable submit so held [`wait`] calls re-check promptly.
    tx: broadcast::Sender<()>,
    /// Count of currently-held long-poll waiters (bounded by [`MAX_WAITERS`]).
    waiters: AtomicUsize,
    /// Per-key sliding-window submit timestamps for the rate limiter.
    rate: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl SubmissionStore {
    /// Open (creating if needed) the store rooted at `root`, recovering the id
    /// counter from the highest id already on disk.
    pub fn open(root: &Path) -> std::io::Result<Arc<Self>> {
        std::fs::create_dir_all(root)?;
        fsync_dir(root)?;
        let max_id = scan_max_id(root);
        let (tx, _) = broadcast::channel(64);
        Ok(Arc::new(SubmissionStore {
            root: root.to_path_buf(),
            next_id: AtomicU64::new(max_id + 1),
            tx,
            waiters: AtomicUsize::new(0),
            rate: Mutex::new(HashMap::new()),
        }))
    }

    /// Subscribe to submit notifications (one receiver per held long-poll).
    fn subscribe(&self) -> broadcast::Receiver<()> {
        self.tx.subscribe()
    }

    /// Check + record the rate limit for `key`. Returns `false` (reject) when the
    /// key has already had [`RATE_MAX`] accepted submits within [`RATE_WINDOW`], or
    /// when the tracking map is full of *other* active keys (fail-closed). On accept
    /// it records `now` so the window slides.
    fn rate_check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut map = self.rate.lock().unwrap_or_else(|p| p.into_inner());
        // Prune this key's expired hits first.
        let entry = map.entry(key.to_string()).or_default();
        while entry.front().is_some_and(|t| now.duration_since(*t) >= RATE_WINDOW) {
            entry.pop_front();
        }
        if entry.len() >= RATE_MAX {
            return false;
        }
        // Bound the map: if it has grown large, drop keys that have fully expired
        // (cheap amortized GC) before admitting a brand-new key.
        if map.len() > RATE_MAX_KEYS && !map.contains_key(key) {
            map.retain(|_, hits| {
                hits.iter().any(|t| now.duration_since(*t) < RATE_WINDOW)
            });
            if map.len() > RATE_MAX_KEYS {
                return false;
            }
        }
        map.entry(key.to_string()).or_default().push_back(now);
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
        let data_bytes = serde_json::to_vec(&data)
            .map_err(|e| SubmitError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
        if data_bytes.len() > MAX_SUBMISSION_BYTES {
            return Err(SubmitError::TooLarge);
        }
        if !self.rate_check(key) {
            return Err(SubmitError::RateLimited);
        }

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
            data,
        };
        self.write_record(&key_dir, id, &record)
            .map_err(SubmitError::Io)?;
        // Notify held waiters only after the record is durable. A send error just
        // means no one is waiting — the submission is on disk for the next poll.
        let _ = self.tx.send(());
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
    pub fn list_since(&self, key: &str, since: u64, max: usize) -> std::io::Result<ListPage> {
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
        let mut out: Vec<Submission> = Vec::new();
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
            if id <= since {
                continue;
            }
            match read_record(&entry.path()) {
                Ok(Some(rec)) if rec.id == id && rec.key == key => out.push(rec),
                Ok(_) => {}
                Err(e) => eprintln!(
                    "glasspad: skipping unreadable submission {}: {e}",
                    entry.path().display()
                ),
            }
        }
        out.sort_by_key(|s| s.id);
        out.truncate(max);
        let cursor = out.last().map(|s| s.id).unwrap_or(since);
        Ok(ListPage {
            submissions: out,
            cursor,
        })
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

    /// Remove submissions older than `retention`, then reap emptied key
    /// directories and leftover `.tmp` staging files. Returns the number removed.
    /// Best-effort per-file; a single failure is logged, not fatal.
    pub fn gc(&self, retention: ChronoDuration) -> std::io::Result<usize> {
        let cutoff = Utc::now() - retention;
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
        Ok(removed)
    }
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
/// `timeout` elapses, whichever first. Bounded by [`MAX_WAITERS`] concurrent
/// holders and by `timeout` (already clamped by the caller). The subscribe-before-
/// check ordering guarantees no lost wakeup: a submit that notifies after we
/// subscribe is received; one that landed before is seen by the initial
/// `list_since`. `Lagged` (missed notifications under load) just triggers a
/// re-check, never a miss.
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
    let deadline = Instant::now() + timeout;
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
        let now = Instant::now();
        if now >= deadline {
            return Ok(WaitOutcome::TimedOut { cursor: since });
        }
        let remaining = deadline - now;
        tokio::select! {
            r = rx.recv() => {
                match r {
                    // A submit fired, or we lagged behind — either way re-check.
                    Ok(()) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    // The sender was dropped (store gone): stop holding.
                    Err(broadcast::error::RecvError::Closed) => {
                        return Ok(WaitOutcome::TimedOut { cursor: since });
                    }
                }
            }
            _ = tokio::time::sleep(remaining) => {}
        }
    }
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
        assert!(store.list_since("other", 0, MAX_LIST).unwrap().submissions.is_empty());
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
        assert_eq!(store2.list_since("abc", 0, MAX_LIST).unwrap().submissions.len(), 2);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn oversize_payload_rejected() {
        let root = tmp_root("big");
        let store = SubmissionStore::open(&root).unwrap();
        let big = "x".repeat(MAX_SUBMISSION_BYTES + 1);
        let err = store
            .submit("abc", "index", "acme", "v1", serde_json::json!({ "f": big }))
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
        let path = root.join("abc").join(format!("{}.json", page.submissions[0].id));
        let mut rec: Submission =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        rec.created_at = Utc::now() - ChronoDuration::days(10);
        std::fs::write(&path, serde_json::to_vec(&rec).unwrap()).unwrap();

        let removed = store.gc(ChronoDuration::days(7)).unwrap();
        assert_eq!(removed, 1);
        assert!(!root.join("abc").exists(), "emptied key dir is reaped");
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
            s.submit("abc", "index", "acme", "v1", serde_json::json!({"ok": true}))
                .unwrap();
        });
        let outcome = wait(store.clone(), "abc".into(), 0, Duration::from_secs(5), MAX_LIST)
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
        store
            .submit("abc", "index", "acme", "v1", serde_json::json!({"n": 1}))
            .unwrap();
        // A submission that landed BEFORE the wait (between arm calls) is returned
        // on the next `since`, never missed.
        let outcome = wait(store, "abc".into(), 0, Duration::from_secs(5), MAX_LIST)
            .await
            .unwrap();
        assert!(matches!(outcome, WaitOutcome::Ready(_)));
        std::fs::remove_dir_all(&root).ok();
    }
}
