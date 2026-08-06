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

/// Parse the pid-file contents into a PID. Rejects empty/whitespace, non-numeric,
/// and `0` (never a real process) — returning the trimmed raw text on failure so
/// the caller can report it. Pure: unit-tested directly.
fn parse_pid(s: &str) -> Result<u32, String> {
    let t = s.trim();
    match t.parse::<u32>() {
        Ok(0) | Err(_) => Err(t.to_string()),
        Ok(pid) => Ok(pid),
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
/// directory if absent.
pub fn write_at(p: &Path, pid: u32) -> Result<(), PidError> {
    if let Some(dir) = p.parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir).map_err(|e| PidError::Io(dir.to_path_buf(), e))?;
    }
    std::fs::write(p, format!("{pid}\n")).map_err(|e| PidError::Io(p.to_path_buf(), e))
}

/// Remove the default pid file **only if** it still records `pid` (ours). A file
/// that a successor has since overwritten with its own PID is left intact, so a
/// server shutting down never deletes its replacement's entry. Best-effort: any
/// error (including no pid file) is ignored — cleanup must never fail loudly.
pub fn remove_if_owned(pid: u32) {
    if let Ok(p) = path() {
        remove_if_owned_at(&p, pid);
    }
}

/// [`remove_if_owned`] against an explicit path (the testable core).
pub fn remove_if_owned_at(p: &Path, pid: u32) {
    if let Ok(s) = std::fs::read_to_string(p)
        && parse_pid(&s).ok() == Some(pid)
    {
        let _ = std::fs::remove_file(p);
    }
}

// --- process primitives (Unix) --------------------------------------------

/// Is `pid` a live process? Uses `kill(pid, 0)`: success or `EPERM` (the process
/// exists but we may not signal it) both mean alive; `ESRCH` means gone. A stale
/// pid file (dead process) is thus reliably distinguished from a running server.
#[cfg(unix)]
pub fn process_alive(pid: u32) -> bool {
    // SAFETY: `kill` with signal 0 performs only the existence/permission check and
    // sends no signal; it has no memory-safety preconditions.
    if unsafe { libc::kill(pid as libc::pid_t, 0) } == 0 {
        return true;
    }
    io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Send `SIGTERM` to `pid` for a graceful stop. The target's signal handler
/// removes its own pid file and exits; `ESRCH`/`EPERM` are surfaced to the caller.
#[cfg(unix)]
pub fn send_term(pid: u32) -> io::Result<()> {
    // SAFETY: `kill` takes a pid + signal number and has no memory-safety
    // preconditions; the result is checked via errno below.
    if unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) } == 0 {
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
        std::env::temp_dir().join(format!("gp-pidfile-{tag}-{}-{}", std::process::id(), tag))
    }

    #[test]
    fn parse_pid_strict() {
        assert_eq!(parse_pid("1234"), Ok(1234));
        assert_eq!(parse_pid("  42\n"), Ok(42)); // trims surrounding whitespace
        assert!(parse_pid("0").is_err()); // 0 is never a real process
        assert!(parse_pid("").is_err());
        assert!(parse_pid("   ").is_err());
        assert!(parse_pid("12x").is_err());
        assert!(parse_pid("-1").is_err());
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
        // Removal is ownership-checked: a different PID does NOT delete it.
        remove_if_owned_at(&p, 9999);
        assert_eq!(read_at(&p).unwrap(), Some(4321), "not ours → left intact");
        // Our PID removes it.
        remove_if_owned_at(&p, 4321);
        assert_eq!(read_at(&p).unwrap(), None);
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
        // PID 999999 is (almost certainly) not a live process → detected dead, so a
        // pid file recording it would be treated as stale, not "already running".
        assert!(!process_alive(999_999));
    }
}
