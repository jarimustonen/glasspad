//! The loopback-server PID file — enabling `glasspad stop`.
//!
//! A single global file at `~/.glasspad/server.pid` records the PID of the
//! currently-running loopback server (`serve` / `create` / `render`). It exists so
//! `glasspad stop` (and any future status probe) can find and signal that process
//! without any network call — the guard on the network surface (the loopback
//! DNS-rebinding Host check in `server.rs`) is untouched by this module. Process
//! management here is purely local: read a file, check a PID with `kill(pid, 0)`,
//! send `SIGTERM`.
//!
//! **Location override.** `GLASSPAD_PID_FILE`, when set to a non-empty path,
//! replaces the default `~/.glasspad/server.pid`. This lets a test (or a caller
//! that runs several isolated servers) point the pid file somewhere hermetic
//! instead of clobbering the user's real one.
//!
//! **Last-writer-wins, never refuse.** Only one PID file exists, so it tracks the
//! most-recently-started loopback server. A *stale* file (the recorded process is
//! dead) is silently reclaimed; a *live* one (a second server is genuinely
//! running, e.g. on another port) is overwritten with a non-fatal warning rather
//! than refusing to start — refusing would make the common pkill-and-restart loop
//! (and legitimate multi-port use) fragile. `stop` therefore targets whichever
//! server last recorded itself.
//!
//! **Ownership-checked removal.** Cleanup (`remove_if_owned`) deletes the file
//! only when it still contains *our* PID, so a server shutting down never deletes
//! the pid file a successor just wrote (the takeover race).

use std::io;
use std::path::{Path, PathBuf};

/// The default pid-file directory + name under the home directory.
const DIR: &str = ".glasspad";
const FILE: &str = "server.pid";

/// The environment override for the pid-file path (see module docs).
pub const PATH_ENV: &str = "GLASSPAD_PID_FILE";

/// A pid-file operation failure, mapped by the CLI to an error envelope.
#[derive(Debug)]
pub enum PidError {
    /// The home directory could not be resolved and no override was set.
    NoHome,
    /// An I/O error touching the pid file (with the path it concerns).
    Io(PathBuf, io::Error),
    /// The pid file exists but does not contain a valid PID (with the raw text).
    Malformed(PathBuf, String),
}

impl std::fmt::Display for PidError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PidError::NoHome => write!(
                f,
                "cannot determine the home directory for the pid file ($HOME unset); \
                 set {PATH_ENV} to an explicit path"
            ),
            PidError::Io(p, e) => write!(f, "pid file {}: {e}", p.display()),
            PidError::Malformed(p, raw) => write!(
                f,
                "pid file {} does not contain a valid PID (found {raw:?})",
                p.display()
            ),
        }
    }
}

/// Resolve the pid-file path: `$GLASSPAD_PID_FILE` if set non-empty, else
/// `~/.glasspad/server.pid`. Errors only when neither is available.
pub fn path() -> Result<PathBuf, PidError> {
    if let Some(over) = std::env::var_os(PATH_ENV)
        && !over.is_empty()
    {
        return Ok(PathBuf::from(over));
    }
    let home = dirs::home_dir().ok_or(PidError::NoHome)?;
    Ok(home.join(DIR).join(FILE))
}

/// The largest PID we will ever store or signal. A Unix `pid_t` is a *signed*
/// 32-bit int, so a value above `i32::MAX` would wrap to a negative number when
/// cast — and `kill()` reads a negative PID as "signal a whole process group"
/// (`-1` = *every* process the caller may signal). Capping at `i32::MAX` here (and
/// re-checking at the syscall in [`checked_pid_t`]) makes that catastrophe
/// unreachable from a corrupt/hostile pid file.
const MAX_PID: u32 = i32::MAX as u32;

/// Parse the pid-file contents into a PID. Rejects empty/whitespace, non-numeric,
/// `0` (never a real process), and anything above [`MAX_PID`] (would cast to a
/// negative `pid_t` and turn a `kill` into a process-group broadcast) — returning
/// the trimmed raw text on failure so the caller can report it. Pure: unit-tested.
fn parse_pid(s: &str) -> Result<u32, String> {
    let t = s.trim();
    match t.parse::<u32>() {
        Ok(pid) if (1..=MAX_PID).contains(&pid) => Ok(pid),
        _ => Err(t.to_string()),
    }
}

/// Read + parse the pid file at the default [`path`]. `Ok(None)` when the file is
/// absent (no server tracked); `Malformed` when it holds junk (surfaced, never
/// silently treated as absent).
pub fn read() -> Result<Option<u32>, PidError> {
    read_at(&path()?)
}

/// [`read`] against an explicit path (the testable core).
pub fn read_at(p: &Path) -> Result<Option<u32>, PidError> {
    match std::fs::read_to_string(p) {
        Ok(s) => parse_pid(&s)
            .map(Some)
            .map_err(|raw| PidError::Malformed(p.to_path_buf(), raw)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(PidError::Io(p.to_path_buf(), e)),
    }
}

/// Write `pid` to the default [`path`], creating `~/.glasspad/` if needed. Returns
/// the path written. Overwrites any existing entry (stale or live — last-writer-
/// wins; the live case is warned by the caller).
pub fn write(pid: u32) -> Result<PathBuf, PidError> {
    let p = path()?;
    write_at(&p, pid)?;
    Ok(p)
}

/// [`write`] against an explicit path (the testable core). Creates the parent
/// directory if absent, then publishes the PID **atomically**: write a
/// same-directory temp file and `rename` it into place, so a concurrent reader
/// (`stop`, or another starting server) never observes a truncated/empty file
/// mid-write — it sees either the old contents or the complete new PID.
pub fn write_at(p: &Path, pid: u32) -> Result<(), PidError> {
    let dir = p.parent().filter(|d| !d.as_os_str().is_empty());
    if let Some(dir) = dir {
        std::fs::create_dir_all(dir).map_err(|e| PidError::Io(dir.to_path_buf(), e))?;
    }
    // The temp file lives in the destination directory (same filesystem → `rename`
    // is atomic and cannot fail with EXDEV) and is process-unique so two concurrent
    // writers never collide on it.
    let tmp = match p.file_name() {
        Some(name) => {
            let mut t = name.to_os_string();
            t.push(format!(".tmp.{}", std::process::id()));
            dir.map(|d| d.join(&t)).unwrap_or_else(|| PathBuf::from(&t))
        }
        None => return Err(PidError::Io(p.to_path_buf(), invalid_path_err())),
    };
    std::fs::write(&tmp, format!("{pid}\n")).map_err(|e| PidError::Io(tmp.clone(), e))?;
    std::fs::rename(&tmp, p).map_err(|e| {
        // Best-effort cleanup of the temp file if the rename could not complete.
        let _ = std::fs::remove_file(&tmp);
        PidError::Io(p.to_path_buf(), e)
    })
}

/// An `InvalidInput` error for a path with no file-name component (e.g. `/`).
fn invalid_path_err() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "path has no file name")
}

/// Remove the default pid file **only if** it still records `pid` (ours), returning
/// whether a file was actually removed. A file a successor has since overwritten
/// with its own PID is left intact — so in the common case a shutting-down server
/// does not delete its replacement's entry. This is a read-then-unlink check, not a
/// true compare-and-delete: a successor that overwrites the file *between* our read
/// and the `remove` can still lose its entry (a microsecond TOCTOU window). That
/// residual race is acceptable here because a missing pid file is self-healing — the
/// next `serve`/`stop` reclaims it — so the failure mode is a harmless re-scan, not
/// data loss. Best-effort: any error (including no pid file) yields `false`.
pub fn remove_if_owned(pid: u32) -> bool {
    match path() {
        Ok(p) => remove_if_owned_at(&p, pid),
        Err(_) => false,
    }
}

/// [`remove_if_owned`] against an explicit path (the testable core). Returns `true`
/// only when the file matched `pid` and was successfully unlinked.
pub fn remove_if_owned_at(p: &Path, pid: u32) -> bool {
    if let Ok(s) = std::fs::read_to_string(p)
        && parse_pid(&s).ok() == Some(pid)
    {
        return std::fs::remove_file(p).is_ok();
    }
    false
}

// --- process primitives (Unix) --------------------------------------------

/// Convert a `u32` PID to a positive `pid_t`, or `None` if it is `0` or would not
/// fit a signed `pid_t` (> [`MAX_PID`]). Defense in depth: `parse_pid` already
/// enforces this range, but every `kill` goes through here so a stray caller can
/// never pass a value that wraps to a negative (process-group / kill-all) PID.
#[cfg(unix)]
fn checked_pid_t(pid: u32) -> Option<libc::pid_t> {
    if (1..=MAX_PID).contains(&pid) {
        Some(pid as libc::pid_t)
    } else {
        None
    }
}

/// Is `pid` a live process? Uses `kill(pid, 0)`: success or `EPERM` (the process
/// exists but we may not signal it) both mean alive; `ESRCH` means gone. A stale
/// pid file (dead process) is thus reliably distinguished from a running server.
/// An out-of-range PID (rejected by [`checked_pid_t`]) is reported not-alive rather
/// than being passed to `kill`.
#[cfg(unix)]
pub fn process_alive(pid: u32) -> bool {
    let Some(pid) = checked_pid_t(pid) else {
        return false;
    };
    // SAFETY: `pid` is a validated positive `pid_t`; `kill` with signal 0 performs
    // only the existence/permission check, sends no signal, and has no
    // memory-safety preconditions.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return true;
    }
    io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Send `SIGTERM` to `pid` for a graceful stop. The target's signal handler
/// removes its own pid file and exits; `ESRCH`/`EPERM` are surfaced to the caller.
/// An out-of-range PID is an `InvalidInput` error — never a wrapped negative PID
/// handed to `kill`.
#[cfg(unix)]
pub fn send_term(pid: u32) -> io::Result<()> {
    let pid = checked_pid_t(pid).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("PID {pid} is outside the valid range 1..={MAX_PID}"),
        )
    })?;
    // SAFETY: `pid` is a validated positive `pid_t`; `kill` has no memory-safety
    // preconditions and the result is checked via errno below.
    if unsafe { libc::kill(pid, libc::SIGTERM) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Non-Unix fallback: process management via signals is unavailable, so liveness
/// cannot be determined (reported dead) and no signal can be sent.
#[cfg(not(unix))]
pub fn process_alive(_pid: u32) -> bool {
    false
}

#[cfg(not(unix))]
pub fn send_term(_pid: u32) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "sending signals is not supported on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_pidfile(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("gp-pidfile-{tag}-{}", std::process::id()))
    }

    #[test]
    fn parse_pid_strict() {
        assert_eq!(parse_pid("1234"), Ok(1234));
        assert_eq!(parse_pid("  42\n"), Ok(42)); // trims surrounding whitespace
        assert_eq!(parse_pid(&MAX_PID.to_string()), Ok(MAX_PID)); // the ceiling is allowed
        assert!(parse_pid("0").is_err()); // 0 is never a real process
        assert!(parse_pid("").is_err());
        assert!(parse_pid("   ").is_err());
        assert!(parse_pid("12x").is_err());
        assert!(parse_pid("-1").is_err());
        // Above i32::MAX would cast to a negative pid_t (kill-group / kill-all) — a
        // corrupt/hostile pid file must never be accepted as a signalable PID.
        assert!(parse_pid("2147483648").is_err()); // i32::MAX + 1
        assert!(parse_pid("4294967295").is_err()); // u32::MAX (would cast to -1)
    }

    #[test]
    fn write_read_remove_roundtrip() {
        let p = temp_pidfile("roundtrip");
        let _ = std::fs::remove_file(&p);
        // Absent → None.
        assert_eq!(read_at(&p).unwrap(), None);
        // Write then read back.
        write_at(&p, 4321).unwrap();
        assert_eq!(read_at(&p).unwrap(), Some(4321));
        // Removal is ownership-checked: a different PID does NOT delete it, and the
        // return value reports that nothing was removed.
        assert!(!remove_if_owned_at(&p, 9999), "not ours → not removed");
        assert_eq!(read_at(&p).unwrap(), Some(4321), "not ours → left intact");
        // Our PID removes it (and reports the removal).
        assert!(remove_if_owned_at(&p, 4321), "ours → removed");
        assert_eq!(read_at(&p).unwrap(), None);
        // A second removal finds nothing → false.
        assert!(!remove_if_owned_at(&p, 4321), "already gone → not removed");
    }

    #[test]
    fn malformed_pidfile_is_surfaced() {
        let p = temp_pidfile("malformed");
        std::fs::write(&p, "not-a-pid").unwrap();
        assert!(matches!(read_at(&p), Err(PidError::Malformed(_, raw)) if raw == "not-a-pid"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn write_creates_missing_parent_dir() {
        let dir = temp_pidfile("mkdir");
        let _ = std::fs::remove_dir_all(&dir);
        let p = dir.join("nested").join("server.pid");
        write_at(&p, 7).unwrap();
        assert_eq!(read_at(&p).unwrap(), Some(7));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn liveness_detects_self_and_a_dead_pid() {
        // Our own process is alive.
        assert!(process_alive(std::process::id()));
        // `MAX_PID` (i32::MAX) is above every platform's pid_max, so it is reliably
        // "no such process" → detected dead, so a pid file recording it is treated
        // as stale, not "already running". (Unlike a small guess like 999999, which
        // can be a live PID on a Linux box with a high pid_max.)
        assert!(!process_alive(MAX_PID));
    }

    #[cfg(unix)]
    #[test]
    fn out_of_range_pid_never_reaches_kill() {
        // Defense in depth for the pid_t-wrap hazard: even if an out-of-range value
        // bypasses `parse_pid`, the process primitives refuse it rather than casting
        // it to a negative (process-group / kill-all) pid_t.
        assert!(!process_alive(u32::MAX));
        assert!(!process_alive(0));
        let err = send_term(u32::MAX).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(send_term(0).is_err());
    }
}
