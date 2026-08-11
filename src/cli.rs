//! The `glasspad` CLI surface (Wave 3a / Phase 3): `serve`, `create`, `open`.
//!
//! Follows the project's AI-first CLI conventions (`AGENTS-AI-FIRST-CLI.md`):
//! strict input validation with informative, actionable errors; a stable,
//! versioned `--json` envelope on every command; errors as a structured envelope
//! on **stderr** with a meaningful exit code (1 = user error, 2 = system error);
//! and no interactive prompts. Paths are plain positional args — no hidden global
//! state, so the commands compose.
//!
//! The commands are three server entry points, a browser opener, and a standalone
//! data helper:
//! * `serve <dir>` drives Phase 2 live directory serving (scan + watch + SSE).
//! * `create <file>` builds a one-artifact space from a single file and serves it
//!   live (its own single-file watch).
//! * `render <file.md>` renders markdown through a reusable template into an
//!   artifact body and serves it live (0.3.0; see `artifact_host::render`).
//! * `build <space> <out>` statically renders a space to self-contained HTML files
//!   (no server, no bind; reuses the scanner + wrap seam — see `crate::build`).
//! * `open <space>` opens a served space's URL in the browser.
//! * `data <file>` parses a legacy CSV/JSON/mbox file to JSON rows (no server).
//!
//! Fragment-vs-full-document detection is **not** re-implemented here: the content
//! route classifies each artifact at serve time (`artifact_host::wrap`), so a
//! file authored either way — full `<!doctype html>` document or bare fragment —
//! is served correctly whether it arrives via `serve` or `create`.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;

use crate::artifact_host::space::{self, ScanError};
use crate::artifact_host::{self, ArtifactHost, render, wrap};
use crate::build::{self, LibMode};
use crate::hosted::auth::{KeyFileError, KeyTable};
use crate::hosted::{self, HostedConfig};
use crate::pidfile::{self, PidError};
use crate::server::{self, RenderTemplate};
use crate::submissions::SubmissionStore;

/// The `--json` schema version (AI-first §10). Bump on any breaking change to an
/// envelope: removed/renamed field, changed type/nullability, or changed meaning.
pub const SCHEMA_VERSION: u32 = 1;

/// The loopback port used when neither `--port` nor `$GLASSPAD_PORT` is set.
pub const DEFAULT_PORT: u16 = 3000;

/// The environment variable that sets the loopback port (AI-first §8: the env name
/// mirrors the `--port` flag).
pub const PORT_ENV: &str = "GLASSPAD_PORT";

// --- port resolution (AI-first §8) ----------------------------------------

/// Resolve the loopback port by AI-first §8 precedence: an explicit `--port` flag >
/// the `$GLASSPAD_PORT` env var > the built-in [`DEFAULT_PORT`]. An invalid
/// `$GLASSPAD_PORT` (empty, non-numeric, or out of the 1-65535 range) is a hard,
/// informative error (§1 — never a silent fallback to the default). The flag is
/// already range-validated by clap, so it is taken verbatim when present.
pub fn resolve_port(flag: Option<u16>, json: bool) -> u16 {
    if let Some(p) = flag {
        return p;
    }
    match std::env::var(PORT_ENV) {
        Err(std::env::VarError::NotPresent) => DEFAULT_PORT,
        Err(std::env::VarError::NotUnicode(_)) => exit_error(
            json,
            1,
            "invalid_port",
            &format!("{PORT_ENV} is set but is not valid UTF-8 (expected an integer 1-65535)"),
            None,
            None,
        ),
        Ok(raw) => match parse_env_port(&raw) {
            Ok(p) => p,
            Err(msg) => exit_error(json, 1, "invalid_port", &msg, Some(raw.trim()), None),
        },
    }
}

/// Parse a `$GLASSPAD_PORT` value into a valid loopback port. Strict (AI-first §1):
/// rejects empty / whitespace-only, non-numeric, and out-of-range (`0` or > 65535)
/// with an informative message naming the offending value. Pure — unit-tested.
fn parse_env_port(raw: &str) -> Result<u16, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err(format!(
            "{PORT_ENV} is set but empty (expected an integer 1-65535)"
        ));
    }
    // Parse into a wider integer first so a syntactically-valid but out-of-range
    // value (e.g. 65536) gets the distinct "out of range" diagnostic rather than
    // collapsing into "not a valid port" (which a u16 parse would give). AI-first
    // §4: name the actual failure so the caller can fix its input precisely.
    match t.parse::<u32>() {
        Ok(v) if (1..=u16::MAX as u32).contains(&v) => Ok(v as u16),
        Ok(v) => Err(format!(
            "{PORT_ENV}={v} is out of range (expected an integer 1-65535)"
        )),
        Err(_) => Err(format!(
            "{PORT_ENV}={t:?} is not a valid port (expected an integer 1-65535)"
        )),
    }
}

// --- pid file (process management) ----------------------------------------

/// Record this process in the loopback-server pid file and arrange its cleanup.
/// Called by `serve`/`create`/`render` **after** a successful bind (so a bind
/// failure never leaves a pid file behind). It:
///
/// * writes our PID (last-writer-wins over any stale *or* live entry — see the
///   `pidfile` module: refusing would make pkill-and-restart and multi-port use
///   fragile), and
/// * installs a SIGINT/SIGTERM handler (Unix) that removes *our* pid file and
///   exits `130`/`143`, so `glasspad stop` (SIGTERM) leaves no stale file.
///
/// Returns any non-fatal warnings (e.g. taking over another live server's entry)
/// for the startup envelope. A write/dir/permission failure is fatal + informative
/// (exit 2) — the issue's fail-closed contract for pid-file errors.
async fn acquire_pidfile(json: bool) -> Vec<String> {
    let mut warnings = Vec::new();
    let me = std::process::id();

    // Detect a pre-existing entry. A live, different PID is a (rare) multi-instance
    // takeover worth a warning; a stale (dead) or malformed/unreadable entry is just
    // overwritten by the write below — the strict surfacing of a bad existing file
    // is `stop`'s job, not a reason to block serving.
    if let Ok(Some(other)) = pidfile::read()
        && other != me
        && pidfile::process_alive(other)
    {
        warnings.push(format!(
            "another glasspad loopback server (pid {other}) is already recorded in the pid \
             file; taking it over (last-writer-wins), so `glasspad stop` will now target \
             pid {me}. Stop the other server manually if it is still needed."
        ));
    }

    // Install the SIGINT/SIGTERM cleanup handler BEFORE publishing our PID, so there
    // is no window in which `stop` can read the pid file and signal us before the
    // handler exists (which would kill us via the default action, leaving a stale
    // file). If a signal arrives before the write, the handler's ownership-checked
    // removal simply finds no file of ours and is a no-op.
    install_signal_cleanup(me);

    if let Err(e) = pidfile::write(me) {
        let exit = match e {
            PidError::Io(..) | PidError::NoHome => 2,
            PidError::Malformed(..) => 1,
        };
        exit_error(
            json,
            exit,
            "pidfile_write_failed",
            &format!(
                "cannot write the loopback-server pid file: {e}. Fix permissions on \
                 ~/.glasspad (it may need creating) or set {} to a writable path.",
                pidfile::PATH_ENV
            ),
            None,
            None,
        );
    }
    warnings
}

/// Install a SIGINT/SIGTERM handler that removes *our* pid file and exits with the
/// signal-conventional code (130 for SIGINT, 143 for SIGTERM — AI-first §12). This
/// is what makes `glasspad stop` (SIGTERM) a clean shutdown that leaves no stale
/// pid file. If handler registration fails, serving continues (the pid file simply
/// becomes stale on exit, which the next `serve`/`stop` reclaims) — never fatal.
#[cfg(unix)]
fn install_signal_cleanup(me: u32) {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = match signal(SignalKind::terminate()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("warning: cannot install SIGTERM handler (pid file may go stale): {e}");
            return;
        }
    };
    let mut intr = match signal(SignalKind::interrupt()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("warning: cannot install SIGINT handler (pid file may go stale): {e}");
            return;
        }
    };
    tokio::spawn(async move {
        let code = tokio::select! {
            _ = term.recv() => 143,
            _ = intr.recv() => 130,
        };
        pidfile::remove_if_owned(me);
        std::process::exit(code);
    });
}

/// Non-Unix: no signal-based cleanup (the pid file is removed on the next start).
#[cfg(not(unix))]
fn install_signal_cleanup(_me: u32) {}

// --- stop -----------------------------------------------------------------

/// `glasspad stop` — stop the running loopback server. Reads the pid file, checks
/// the recorded process is actually alive, and sends `SIGTERM` (the server traps it
/// to remove its own pid file and exit cleanly). This targets a LOCAL process via
/// the pid file + a signal — it makes no network call, so the loopback DNS-rebinding
/// Host guard in `server.rs` is entirely untouched.
///
/// Fail-closed + informative (AI-first §1/§4): with no server running — no pid file,
/// or a *stale* one whose recorded process is dead — it reports `no_running_server`
/// (exit 1) rather than a silent no-op, cleaning a stale file it finds. A permission
/// or I/O failure is a system error (exit 2). On success it does **not** delete the
/// pid file: the signaled server removes its own entry (ownership-checked), which
/// avoids racing a fast restart.
///
/// Signal-based process management is Unix-only; on a non-Unix platform `stop`
/// reports `unsupported_platform` (exit 2) rather than pretending — the non-Unix
/// liveness stub always reports "dead", so falling through would falsely claim
/// "no running server" while a server kept running.
#[cfg(unix)]
pub fn stop(json: bool) {
    let pid = match pidfile::read() {
        Ok(Some(p)) => p,
        Ok(None) => exit_error(
            json,
            1,
            "no_running_server",
            &format!(
                "no running glasspad loopback server (no pid file at {}). \
                 Start one with `glasspad serve <dir>`.",
                pid_path_display()
            ),
            None,
            None,
        ),
        Err(e) => {
            // A malformed pid file is a fixable user/state error (1); an I/O or
            // no-home failure is a system error (2).
            let exit = match e {
                PidError::Malformed(..) => 1,
                PidError::Io(..) | PidError::NoHome => 2,
            };
            exit_error(json, exit, "pidfile_unreadable", &e.to_string(), None, None);
        }
    };

    if !pidfile::process_alive(pid) {
        // Stale: the recorded process is gone. Clean the file (if still ours) and
        // report no running server — the issue's "stale is not already-running" rule.
        no_running_server_stale(json, pid, "that process is not alive");
    }

    match pidfile::send_term(pid) {
        Ok(()) => emit_stopped(json, pid),
        Err(e) if e.raw_os_error() == Some(libc::ESRCH) => {
            // The process exited between the liveness check and the signal — treat
            // it as no running server and clean the (now stale) pid file.
            no_running_server_stale(json, pid, "it exited before it could be signaled");
        }
        Err(e) => exit_error(
            json,
            2,
            "stop_failed",
            &format!("cannot signal pid {pid}: {e}"),
            None,
            None,
        ),
    }
}

/// Non-Unix: `stop` cannot deliver a signal, so it fails explicitly rather than
/// misreporting a running server as stopped.
#[cfg(not(unix))]
pub fn stop(json: bool) {
    exit_error(
        json,
        2,
        "unsupported_platform",
        "glasspad stop is only supported on Unix platforms (it stops the server with SIGTERM)",
        None,
        None,
    );
}

/// Exit with `no_running_server` for a stale-pid situation, cleaning our entry if it
/// is still ours. The removal is ownership-checked (`remove_if_owned` returns whether
/// it actually removed anything), so the message never claims a removal that did not
/// happen — in the rare takeover race the file now belongs to a live successor and is
/// left intact, which the message states truthfully.
#[cfg(unix)]
fn no_running_server_stale(json: bool, pid: u32, why: &str) -> ! {
    let cleanup = if pidfile::remove_if_owned(pid) {
        "Removed the stale pid file."
    } else {
        "The pid file now records a different server (or was already gone); left it intact."
    };
    exit_error(
        json,
        1,
        "no_running_server",
        &format!(
            "no running glasspad loopback server: the pid file recorded pid {pid}, but {why}. \
             {cleanup}"
        ),
        None,
        None,
    );
}

/// The pid-file path for messages (best-effort — falls back to the literal default
/// if the home directory cannot be resolved).
#[cfg(unix)]
fn pid_path_display() -> String {
    pidfile::path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "~/.glasspad/server.pid".to_string())
}

/// Print the `stop` result envelope. Success is a terminal result (not long-running),
/// so `--json` emits a one-line result object; text prints a concise confirmation.
/// `stopped: true` means SIGTERM was delivered to the server (the `signal` field
/// names it); the server then shuts down and removes its own pid file.
#[cfg(unix)]
fn emit_stopped(json: bool, pid: u32) {
    if json {
        let payload = json!({
            "schema_version": SCHEMA_VERSION,
            "stopped": true,
            "pid": pid,
            "signal": "SIGTERM",
            "warnings": [],
        });
        emit_json_line(&payload);
    } else {
        println!("stopped glasspad loopback server (pid {pid}, SIGTERM)");
    }
}

// --- Error contract -------------------------------------------------------

/// Emit a structured error and exit. Under `--json` the AI-first §10 error
/// envelope goes to **stderr** (stdout stays the data channel); otherwise a
/// `error: <message>` line. `exit_code` is 1 for user error, 2 for a system/IO
/// failure the caller cannot fix by correcting its input.
pub fn exit_error(
    json: bool,
    exit_code: i32,
    code: &str,
    message: &str,
    invalid_value: Option<&str>,
    expected: Option<Vec<String>>,
) -> ! {
    if json {
        let mut err = serde_json::Map::new();
        err.insert("code".into(), json!(code));
        err.insert("message".into(), json!(message));
        if let Some(v) = invalid_value {
            err.insert("invalid_value".into(), json!(v));
        }
        if let Some(e) = expected {
            err.insert("expected".into(), json!(e));
        }
        let payload = json!({ "schema_version": SCHEMA_VERSION, "error": err });
        // Never emit a blank line: if the (statically-serializable) envelope somehow
        // fails to serialize, fall back to the text form so the message is not lost.
        match serde_json::to_string(&payload) {
            Ok(s) => eprintln!("{s}"),
            Err(_) => eprintln!("error: {message}"),
        }
    } else {
        eprintln!("error: {message}");
    }
    std::process::exit(exit_code);
}

/// Map a scanner rejection to a stable error `code` and exit code, then emit it.
/// The human message is the `ScanError` `Display` (already informative and, for
/// the symlink/reserved/collision cases, keyword-greppable by `test-security.sh`).
fn exit_scan_error(e: &ScanError, json: bool) -> ! {
    let (code, exit) = match e {
        ScanError::NotADir(_) => ("not_a_directory", 1),
        ScanError::BadSpaceName(_) => ("invalid_space_name", 1),
        ScanError::Io(_, _) => ("io_error", 2),
        ScanError::Symlink(_) => ("symlink_rejected", 1),
        ScanError::Escapes(_) => ("path_escapes_root", 1),
        ScanError::ReservedSlug(_, _) => ("reserved_slug", 1),
        ScanError::BadSlug(_, _) => ("invalid_slug", 1),
        ScanError::DuplicateSlug(_, _) => ("duplicate_slug", 1),
        ScanError::FileTooLarge(_, _) => ("file_too_large", 1),
        ScanError::SpaceTooLarge(_) => ("space_too_large", 1),
        ScanError::TooManyEntries(_) => ("too_many_entries", 1),
        ScanError::UnsupportedFileType(_) => ("unsupported_file_type", 1),
        ScanError::ManifestTooLarge(_, _) => ("manifest_too_large", 1),
        ScanError::NotUtf8(_) => ("not_utf8", 1),
        ScanError::BadAssetName(_) => ("invalid_asset_name", 1),
        ScanError::Manifest(_, _) => ("invalid_manifest", 1),
    };
    exit_error(json, exit, code, &e.to_string(), None, None);
}

/// Print a JSON envelope line to stdout and flush it. The flush matters for the
/// long-running `serve`/`create`: their startup envelope must reach a piped
/// consumer *before* the process blocks serving, not sit in a block buffer.
fn emit_json_line(payload: &serde_json::Value) {
    use std::io::Write;
    let s = serde_json::to_string(payload).unwrap_or_default();
    println!("{s}");
    let _ = std::io::stdout().flush();
}

// --- version --------------------------------------------------------------

/// `glasspad version` (and `glasspad --version` / `-V`) — report the installed
/// CLI version so tooling (the homebase fleet-updater) can version-gate installs,
/// matching the sibling CLIs (`issuectl --version`, `ossctl version`,
/// `orchestratectl version`).
///
/// Under `--json`, emit the AI-first §10 envelope with the version payload
/// **nested under `data`** — `{schema_version, data: {name, version, commit},
/// warnings}` — the same shape orchestratectl/ossctl `version` use, so the
/// cross-tool fleet-updater reads `.data.version` uniformly across every tool
/// rather than special-casing glasspad. Otherwise a plain `glasspad <version>`
/// line on stdout (the data channel). Both the subcommand and the `--version` /
/// `-V` flag route here (main dispatches the flag manually), so all three honor
/// `--json` identically.
///
/// `version`/`name` are the compile-time `CARGO_PKG_VERSION`/`CARGO_PKG_NAME`,
/// the single source of truth shared with `Cargo.toml`. `commit` is the build
/// provenance: `build.rs` resolves the 12-char short SHA of the repository HEAD
/// and emits it under the internal carrier `GLASSPAD_COMMIT` at compile time
/// when built inside this crate's git checkout, and `option_env!` reads it here.
/// (The public `GLASSPAD_BUILD_COMMIT` override input is consumed and validated
/// by `build.rs`, never read here — a bare `option_env!` reads the ambient
/// compile environment, so reading that name directly would bypass validation.)
/// Outside a checkout (a crates.io tarball / `cargo install` with no `.git`, or
/// git missing) the build script emits nothing, so `commit` reports `null`,
/// never a bogus or partial hash. It is the HEAD commit, not a guarantee the
/// binary matches that tree (a dirty working tree still reports its HEAD).
pub fn version(json: bool) {
    let name = env!("CARGO_PKG_NAME");
    let ver = env!("CARGO_PKG_VERSION");
    let commit = option_env!("GLASSPAD_COMMIT");
    if json {
        let payload = json!({
            "schema_version": SCHEMA_VERSION,
            "data": {
                "name": name,
                "version": ver,
                // `Option<&str>` → a JSON string or `null`; the key is always
                // present so a strict consumer never hits a missing-field error.
                "commit": commit,
            },
            // Present (empty) for cross-command uniformity: callers read
            // `warnings` unconditionally across every envelope.
            "warnings": [],
        });
        emit_json_line(&payload);
    } else {
        println!("{name} {ver}");
    }
}

// --- serve ----------------------------------------------------------------

/// Build the loopback [`ArtifactHost`], attaching the return-channel submission
/// store when it can be opened. A store that fails to open (permissions, disk) is
/// a **warning**, not fatal: serving pages must not depend on the return channel,
/// so the host comes up with no submission store (submit endpoints then answer
/// `503 return_channel_unavailable`).
fn loopback_host(port: u16) -> Arc<ArtifactHost> {
    let mut host = ArtifactHost::new(port);
    if let Some(store) = loopback_submissions(port) {
        host = host.with_submissions(store);
    }
    Arc::new(host)
}

/// Open the per-port loopback submission store under the state dir
/// (`$GLASSPAD_STATE_DIR`, else `~/.glasspad`) `submissions/<port>/`. Per-port so
/// concurrent `serve`s on different ports never share a channel; the matching
/// `await-submission` reaches it over loopback HTTP, so it needs no path itself.
fn loopback_submissions(port: u16) -> Option<Arc<SubmissionStore>> {
    let base = std::env::var_os("GLASSPAD_STATE_DIR")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".glasspad")))?;
    let dir = base.join("submissions").join(port.to_string());
    match SubmissionStore::open(&dir) {
        Ok(store) => Some(store),
        Err(e) => {
            eprintln!(
                "warning: return channel disabled — cannot open {}: {e}",
                dir.display()
            );
            None
        }
    }
}

/// `glasspad serve [dir]` — serve a live directory as a space, or (with no dir)
/// the built-in fixtures. Binds loopback, then blocks serving until killed.
pub async fn serve(dir: Option<PathBuf>, port: u16, json: bool) {
    let host = loopback_host(port);

    // (name, nav slugs, home) when a live directory is served; None = fixtures.
    let live: Option<(String, Vec<String>, Option<String>)> = match &dir {
        Some(d) => match server::scan_named(d) {
            Ok((name, snap)) => {
                let sp = snap.space(&name).expect("scanned space is present");
                let info = (name.clone(), sp.slugs(), sp.home.clone());
                host.swap(snap);
                Some(info)
            }
            Err(e) => exit_scan_error(&e, json),
        },
        None => None,
    };

    // Bind before announcing: a port collision is surfaced as an error, and the
    // startup envelope is only printed once the port is actually held.
    let listener = match server::bind_loopback(port).await {
        Ok(l) => l,
        Err(e) => exit_error(
            json,
            2,
            "bind_failed",
            &format!("cannot bind 127.0.0.1:{port}: {e}"),
            Some(&port.to_string()),
            None,
        ),
    };

    // Record this process (post-bind, so a bind failure leaves no pid file) and
    // arrange clean SIGTERM/SIGINT shutdown; a write/permission failure is fatal here.
    let pid_warnings = acquire_pidfile(json).await;

    if let Some(d) = dir {
        server::spawn_watcher(host.clone(), d);
    }
    emit_serving(json, port, live.as_ref(), pid_warnings);

    let app = server::build_app_with_host(port, host);
    if let Err(e) = server::serve_on(listener, app).await {
        // A mid-run failure exits without hitting the signal handler; drop our pid
        // file so it does not linger stale.
        pidfile::remove_if_owned(std::process::id());
        exit_error(
            json,
            2,
            "serve_failed",
            &format!("server stopped with an error: {e}"),
            None,
            None,
        );
    }
}

/// Print the `serve` startup envelope. `--json` → one line to stdout (the data
/// channel); text → a line to stderr (a long-running process's stdout stays free
/// for a caller that pipes it). The command is long-running, so this is a startup
/// announcement, not a terminal result — the server then runs until killed. `pid`
/// is included so a caller knows exactly what `glasspad stop` will target; extra
/// (pid-file takeover) warnings are folded into the envelope's `warnings`.
fn emit_serving(
    json: bool,
    port: u16,
    live: Option<&(String, Vec<String>, Option<String>)>,
    mut warnings: Vec<String>,
) {
    let pid = std::process::id();
    match live {
        Some((name, slugs, home)) => {
            let url = format!("http://127.0.0.1:{port}/{name}/");
            if json {
                let payload = json!({
                    "schema_version": SCHEMA_VERSION,
                    "serving": true,
                    "port": port,
                    "pid": pid,
                    "space": name,
                    "url": url,
                    "artifacts": slugs,
                    "home": home,
                    "warnings": warnings,
                });
                emit_json_line(&payload);
            } else {
                for w in &warnings {
                    eprintln!("warning: {w}");
                }
                eprintln!(
                    "glasspad serving space '{name}' at {url} ({} artifact{}, pid {pid})",
                    slugs.len(),
                    if slugs.len() == 1 { "" } else { "s" }
                );
            }
        }
        None => {
            let url = format!("http://127.0.0.1:{port}/");
            // The fixtures caveat leads; any pid-file warning follows it.
            warnings.insert(
                0,
                "no directory given: serving built-in fixtures only; \
                 pass a directory to serve a space"
                    .to_string(),
            );
            if json {
                let payload = json!({
                    "schema_version": SCHEMA_VERSION,
                    "serving": true,
                    "port": port,
                    "pid": pid,
                    "space": serde_json::Value::Null,
                    "url": url,
                    "artifacts": [],
                    "home": serde_json::Value::Null,
                    "warnings": warnings,
                });
                emit_json_line(&payload);
            } else {
                for w in &warnings {
                    eprintln!("warning: {w}");
                }
                eprintln!("glasspad serving built-in fixtures at {url} (pid {pid})");
            }
        }
    }
}

// --- create ---------------------------------------------------------------

/// `glasspad create <file> [--name <space>]` — build a one-artifact space from a
/// single file and serve it live (a single-file watch reloads on edit). The space
/// name defaults to the file stem (validated) and can be overridden with `--name`.
pub async fn create(file: PathBuf, name: Option<String>, port: u16, json: bool) {
    let (space_name, html) = load_single_file(&file, name.as_deref(), json);
    // Report which authoring level was detected — the same classifier the content
    // route uses to decide wrap-vs-verbatim (design.md §4 / plan §4).
    let kind = if wrap::is_fragment(&html) {
        "fragment"
    } else {
        "full-document"
    };

    let host = loopback_host(port);
    host.swap(server::one_artifact_snapshot(&space_name, html));

    let listener = match server::bind_loopback(port).await {
        Ok(l) => l,
        Err(e) => exit_error(
            json,
            2,
            "bind_failed",
            &format!("cannot bind 127.0.0.1:{port}: {e}"),
            Some(&port.to_string()),
            None,
        ),
    };

    let pid_warnings = acquire_pidfile(json).await;
    server::spawn_file_watcher(host.clone(), file, space_name.clone());
    emit_created(json, port, &space_name, kind, pid_warnings);

    let app = server::build_app_with_host(port, host);
    if let Err(e) = server::serve_on(listener, app).await {
        pidfile::remove_if_owned(std::process::id());
        exit_error(
            json,
            2,
            "serve_failed",
            &format!("server stopped with an error: {e}"),
            None,
            None,
        );
    }
}

/// Validate + read the single file `create` serves, resolving the space name.
/// Strict (AI-first §1): a missing path, a directory, a non-regular / oversize /
/// non-UTF-8 file, or an un-derivable/invalid space name each exits with an
/// informative envelope rather than a silent fixup. Returns `(space_name, html)`.
fn load_single_file(file: &Path, name_override: Option<&str>, json: bool) -> (String, String) {
    // Validate the space name FIRST (AI-first §1 fail-fast): the name comes from
    // `--name` or the file stem — neither needs the file contents — so an
    // immediately-detectable argument error is reported before any file I/O.
    let space_name = resolve_space_name(file, name_override, json);

    // `metadata` follows a symlink: the user named this file explicitly, so a
    // symlink to their own file is served (unlike a directory scan, where a
    // symlink can smuggle a file in from outside the space and is rejected).
    let meta = match std::fs::metadata(file) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => exit_error(
            json,
            1,
            "no_such_path",
            &format!("no such file: {}", file.display()),
            Some(&file.display().to_string()),
            None,
        ),
        Err(e) => exit_error(
            json,
            2,
            "io_error",
            &format!("cannot read {}: {e}", file.display()),
            None,
            None,
        ),
    };
    if meta.is_dir() {
        exit_error(
            json,
            1,
            "not_a_file",
            &format!(
                "{} is a directory; `create` takes a single file — use `serve` for a directory",
                file.display()
            ),
            Some(&file.display().to_string()),
            None,
        );
    }
    if !meta.is_file() {
        exit_error(
            json,
            1,
            "not_a_file",
            &format!(
                "{} is not a regular file (FIFOs, sockets, and devices are not servable)",
                file.display()
            ),
            None,
            None,
        );
    }
    if meta.len() > space::MAX_FILE_BYTES {
        exit_error(
            json,
            1,
            "file_too_large",
            &format!(
                "{} is {} bytes, over the {}-byte per-file limit",
                file.display(),
                meta.len(),
                space::MAX_FILE_BYTES
            ),
            None,
            None,
        );
    }

    // Bounded read: cap the allocation at `MAX_FILE_BYTES + 1` so a file that grows
    // past the limit between the stat above and the read (a concurrent writer)
    // cannot make us allocate an unbounded buffer before the size recheck fires.
    let bytes = match read_capped(file, space::MAX_FILE_BYTES) {
        Ok(b) => b,
        Err(e) => exit_error(
            json,
            2,
            "io_error",
            &format!("cannot read {}: {e}", file.display()),
            None,
            None,
        ),
    };
    if bytes.len() as u64 > space::MAX_FILE_BYTES {
        exit_error(
            json,
            1,
            "file_too_large",
            &format!(
                "{} exceeds the {}-byte per-file limit",
                file.display(),
                space::MAX_FILE_BYTES
            ),
            None,
            None,
        );
    }
    let html = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => exit_error(
            json,
            1,
            "not_utf8",
            &format!(
                "{} is not valid UTF-8 (artifacts must be UTF-8 HTML)",
                file.display()
            ),
            None,
            None,
        ),
    };

    (space_name, html)
}

/// Resolve + validate the space name for `create`: the `--name` override, else the
/// file stem. Same grammar the router and scanner enforce, so `create` can never
/// mint a name they would reject. Exits with an informative envelope on failure.
fn resolve_space_name(file: &Path, name_override: Option<&str>, json: bool) -> String {
    let (from_flag, raw_name) = match name_override {
        Some(n) => (true, n.to_string()),
        None => (
            false,
            file.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string(),
        ),
    };
    if artifact_host::valid_space(&raw_name) {
        return raw_name;
    }
    let message = if from_flag {
        format!(
            "invalid --name {raw_name:?}: a space name must be lowercase [a-z0-9-], \
             start alphanumeric, be ≤64 chars, and not be reserved ({})",
            artifact_host::RESERVED.join(", ")
        )
    } else {
        format!(
            "cannot derive a valid space name from {}: {raw_name:?} is not a valid name \
             (lowercase [a-z0-9-], start alphanumeric, ≤64 chars, not reserved: {}). \
             Pass --name <space> to set one explicitly.",
            file.display(),
            artifact_host::RESERVED.join(", ")
        )
    };
    // No `expected` list: the space grammar is not a finite enum, and the reserved
    // names are a *deny* list — surfacing them under `expected` (an allowlist, per
    // AI-first §10) would mislead a caller into retrying with a reserved name. The
    // message already spells out the grammar + reserved set.
    exit_error(
        json,
        1,
        "invalid_space_name",
        &message,
        Some(&raw_name),
        None,
    );
}

/// Read at most `max + 1` bytes of `file` into memory (a bounded allocation). The
/// caller treats a returned length `> max` as over-limit; the `+1` lets it detect
/// "exactly at the cap vs. over" without ever buffering an unbounded file.
fn read_capped(file: &Path, max: u64) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let f = std::fs::File::open(file)?;
    let mut buf = Vec::new();
    f.take(max + 1).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Print the `create` startup envelope (mirrors [`emit_serving`], plus the single
/// slug and the detected authoring `kind`). `pid` names what `stop` targets;
/// `warnings` carries any pid-file takeover note.
fn emit_created(json: bool, port: u16, space: &str, kind: &str, warnings: Vec<String>) {
    let url = format!("http://127.0.0.1:{port}/{space}/");
    let pid = std::process::id();
    if json {
        let payload = json!({
            "schema_version": SCHEMA_VERSION,
            "serving": true,
            "port": port,
            "pid": pid,
            "space": space,
            "slug": server::SINGLE_SLUG,
            "home": server::SINGLE_SLUG,
            "url": url,
            "kind": kind,
            "warnings": warnings,
        });
        emit_json_line(&payload);
    } else {
        for w in &warnings {
            eprintln!("warning: {w}");
        }
        eprintln!("glasspad serving '{space}' ({kind}) at {url} (pid {pid})");
    }
}

// --- render (markdown + reusable template) --------------------------------

/// `glasspad render <markdown-file> [--template <ref>] [--name <space>]` — render
/// a markdown body through a referenced reusable template into a hosted artifact
/// and serve it live (a re-render on every edit of the markdown — or, for a file
/// template, of the template — reloads the browser).
///
/// The template governs **only the artifact body** (`markdown-template-render`
/// decided model): it is spliced into the body via the same content-route seam a
/// `create`d fragment uses (`wrap::render_artifact` → `base.css` + `bridge.js`
/// under the frozen artifact CSP), so it can never touch the trusted shell, CSP,
/// Trusted Types, nav, or the sandbox. See `render` module docs for the boundary
/// argument.
///
/// Strict validation + a stable `--json` envelope, per AGENTS-AI-FIRST-CLI.md.
pub async fn render(
    file: PathBuf,
    template_ref: Option<String>,
    name: Option<String>,
    port: u16,
    json: bool,
) {
    // Validate the space name FIRST (fail-fast §1): it comes from `--name` or the
    // markdown file stem, neither of which needs file contents.
    let space_name = resolve_space_name(&file, name.as_deref(), json);

    // Read + validate the markdown source (same strict checks as `create`).
    let markdown = read_capped_utf8_file(&file, "markdown", "no_such_path", json);

    // Resolve the template reference to its source string + the watcher handle.
    let (template, template_str, kind, label) = resolve_template(template_ref.as_deref(), json);

    // Render markdown + template into the artifact body. A template that lost its
    // single `{{content}}` placeholder is a user error (§1), reported informatively.
    let body = match render::render_to_body(&markdown, &template_str) {
        Ok(b) => b,
        Err(e) => exit_error(
            json,
            1,
            "invalid_template",
            &e.to_string(),
            Some(&label),
            None,
        ),
    };
    // Bound the generated body to the same per-artifact limit `create`/`serve`
    // enforce (rendering can amplify markup past the input cap).
    let body = match server::enforce_body_cap(body) {
        Ok(b) => b,
        Err(msg) => exit_error(json, 1, "rendered_output_too_large", &msg, None, None),
    };

    // A file template that renders a FULL document (opens with `<!doctype>`/`<html>`)
    // is served verbatim — it forgoes the fragment wrap, so it loses the auto-linked
    // `base.css` (incl. the `.gp-prose` theme) and injected `bridge.js` (live reload
    // in-frame). Not a security issue (the `_c` response CSP/sandbox are unchanged),
    // but a footgun worth a non-fatal warning so the author isn't surprised.
    let mut warnings: Vec<String> = Vec::new();
    if wrap::is_full_document(&body) {
        warnings.push(
            "the template renders a full HTML document (opens with <!doctype>/<html>): \
             it is served verbatim, so glasspad does NOT link base.css (the .gp-prose \
             theme) or inject bridge.js (in-frame live reload). Use a fragment template \
             (e.g. the built-in prose/dashboard) to keep those, or link base.css yourself."
                .to_string(),
        );
    }

    let host = loopback_host(port);
    host.swap(server::one_artifact_snapshot(&space_name, body));

    let listener = match server::bind_loopback(port).await {
        Ok(l) => l,
        Err(e) => exit_error(
            json,
            2,
            "bind_failed",
            &format!("cannot bind 127.0.0.1:{port}: {e}"),
            Some(&port.to_string()),
            None,
        ),
    };

    warnings.extend(acquire_pidfile(json).await);
    server::spawn_render_watcher(host.clone(), file, template, space_name.clone());
    emit_rendered(json, port, &space_name, &label, kind, warnings);

    let app = server::build_app_with_host(port, host);
    if let Err(e) = server::serve_on(listener, app).await {
        pidfile::remove_if_owned(std::process::id());
        exit_error(
            json,
            2,
            "serve_failed",
            &format!("server stopped with an error: {e}"),
            None,
            None,
        );
    }
}

/// Resolve `--template <ref>` (default `prose`) to `(watcher handle, source string,
/// kind, label)`. **Resolution rule:** an exact built-in name (`prose` /
/// `dashboard`) resolves to that built-in; **anything else** is a filesystem path
/// to a template file (read strictly). Built-in names contain no `/` or `.`, so a
/// local file literally named `prose` is reachable as `./prose` (≠ `"prose"` → a
/// path) — unambiguous. `kind` is `"builtin"`/`"file"` for the envelope; `label` is
/// the reference echoed back (the name or the path).
fn resolve_template(
    template_ref: Option<&str>,
    json: bool,
) -> (RenderTemplate, String, &'static str, String) {
    let reference = template_ref.unwrap_or(render::DEFAULT_TEMPLATE);
    if let Some(builtin) = render::builtin_template(reference) {
        return (
            RenderTemplate::Builtin(builtin),
            builtin.to_string(),
            "builtin",
            reference.to_string(),
        );
    }
    // A filesystem path to a template file. A *bare* name (no `/`, no `.`) that is
    // neither a built-in nor an existing file is almost certainly a mistyped
    // built-in, not a path — surface the built-in allowlist rather than a bare
    // "no such file" (AI-first §10: an `expected` set on a fixed-enum-like arg).
    let path = PathBuf::from(reference);
    let looks_like_path = reference.contains('/') || reference.contains('.');
    if !looks_like_path && !path.exists() {
        exit_error(
            json,
            1,
            "unknown_template",
            &format!(
                "unknown template {reference:?}: expected a built-in ({}) or a path to a \
                 template file (e.g. ./my-template.html)",
                render::BUILTIN_NAMES.join(", ")
            ),
            Some(reference),
            Some(
                render::BUILTIN_NAMES
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            ),
        );
    }
    let content = read_capped_utf8_file(&path, "template", "template_not_found", json);
    (
        RenderTemplate::File(path),
        content,
        "file",
        reference.to_string(),
    )
}

/// Read + validate a UTF-8 source file (markdown or template), bounded to the
/// per-file cap. Strict like `create` (fail-fast §1): a missing path, a directory,
/// a non-regular / oversize / non-UTF-8 file each exits with an informative
/// envelope rather than a silent fixup. `noun` names the file kind in messages;
/// `missing_code` is the stable `code` for a not-found path (so a missing template
/// reports `template_not_found`, a missing markdown `no_such_path`).
fn read_capped_utf8_file(file: &Path, noun: &str, missing_code: &str, json: bool) -> String {
    let meta = match std::fs::metadata(file) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => exit_error(
            json,
            1,
            missing_code,
            &format!("no such {noun} file: {}", file.display()),
            Some(&file.display().to_string()),
            None,
        ),
        Err(e) => exit_error(
            json,
            2,
            "io_error",
            &format!("cannot read {}: {e}", file.display()),
            None,
            None,
        ),
    };
    if meta.is_dir() {
        exit_error(
            json,
            1,
            "not_a_file",
            &format!(
                "{} is a directory; a {noun} must be a single file",
                file.display()
            ),
            Some(&file.display().to_string()),
            None,
        );
    }
    if !meta.is_file() {
        exit_error(
            json,
            1,
            "not_a_file",
            &format!(
                "{} is not a regular file (FIFOs, sockets, and devices are not supported)",
                file.display()
            ),
            None,
            None,
        );
    }
    if meta.len() > space::MAX_FILE_BYTES {
        exit_error(
            json,
            1,
            "file_too_large",
            &format!(
                "{} is {} bytes, over the {}-byte per-file limit",
                file.display(),
                meta.len(),
                space::MAX_FILE_BYTES
            ),
            None,
            None,
        );
    }
    let bytes = match read_capped(file, space::MAX_FILE_BYTES) {
        Ok(b) => b,
        Err(e) => exit_error(
            json,
            2,
            "io_error",
            &format!("cannot read {}: {e}", file.display()),
            None,
            None,
        ),
    };
    if bytes.len() as u64 > space::MAX_FILE_BYTES {
        exit_error(
            json,
            1,
            "file_too_large",
            &format!(
                "{} exceeds the {}-byte per-file limit",
                file.display(),
                space::MAX_FILE_BYTES
            ),
            None,
            None,
        );
    }
    match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => exit_error(
            json,
            1,
            "not_utf8",
            &format!(
                "{} is not valid UTF-8 ({noun} must be UTF-8)",
                file.display()
            ),
            None,
            None,
        ),
    }
}

/// Print the `render` startup envelope (mirrors [`emit_created`], plus the resolved
/// template + its kind, and any non-fatal `warnings`).
fn emit_rendered(
    json: bool,
    port: u16,
    space: &str,
    template: &str,
    kind: &str,
    warnings: Vec<String>,
) {
    let url = format!("http://127.0.0.1:{port}/{space}/");
    let pid = std::process::id();
    if json {
        let payload = json!({
            "schema_version": SCHEMA_VERSION,
            "serving": true,
            "port": port,
            "pid": pid,
            "space": space,
            "slug": server::SINGLE_SLUG,
            "home": server::SINGLE_SLUG,
            "url": url,
            "template": template,
            "template_kind": kind,
            "warnings": warnings,
        });
        emit_json_line(&payload);
    } else {
        for w in &warnings {
            eprintln!("warning: {w}");
        }
        eprintln!(
            "glasspad serving '{space}' (rendered via {kind} template '{template}') at {url} \
             (pid {pid})"
        );
    }
}

// --- build (static render) ------------------------------------------------

/// `glasspad build <space> <out> [--shared-libs] [--force] [--dry-run]` —
/// statically render a space directory to self-contained HTML files (no server,
/// no bind). Reuses the same security-checked scanner + wrap seam `serve` uses,
/// producing the same wrapped pages the content route would serve, written to
/// `<out>` for an offline docsite / external preview transport (see `build` docs).
///
/// Strict + fail-fast (AI-first §1): a symlink / traversal / reserved-slug /
/// oversize input is refused by the scanner before anything is written, and a
/// non-empty `<out>` is refused unless `--force` (§3 — a potentially-overwriting
/// write opts in explicitly). `--dry-run` (§11) validates + plans and prints the
/// file list without touching the filesystem.
pub fn build(
    space_dir: PathBuf,
    out: PathBuf,
    shared_libs: bool,
    force: bool,
    dry_run: bool,
    json: bool,
) {
    let mode = if shared_libs {
        LibMode::SharedLibs
    } else {
        LibMode::SelfContained
    };

    // Scan the space with the SAME scanner `serve` uses: a symlink, path
    // traversal, reserved slug, collision, or oversize file is refused here just
    // as on the server path (AI-first §1), before any output is written.
    let (name, snap) = match server::scan_named(&space_dir) {
        Ok(x) => x,
        Err(e) => exit_scan_error(&e, json),
    };
    let space = snap.space(&name).expect("scanned space is present");
    let home = space.home.clone();
    let slugs = space.slugs();

    // Refuse an output that would overwrite or pollute the source space: writing
    // INTO (or AT) the scanned directory would either clobber source files or seed
    // the next scan with generated `.html`/`_gp` output. Checked before planning so
    // it surfaces in --dry-run too (a read-only, non-mutating validation, §11).
    guard_out_not_in_space(&space_dir, &out, json);

    // Plan every output file (pure — no filesystem writes yet).
    let files = build::plan(space, home.as_deref(), mode);
    let index = files
        .iter()
        .any(|f| f.rel_path == "index.html")
        .then(|| "index.html".to_string());

    // Non-fatal caveats every build carries (AI-first §10 warnings go in the
    // stdout payload / on stderr in text mode). The security note is standing: the
    // static output is NOT the live host's sandbox.
    let mut warnings: Vec<String> = vec![
        "static output is NOT sandboxed like the live host (no null-origin iframe, no \
         per-response CSP) and has no trusted nav shell: cross-artifact bridge navigation \
         and extensionless relative links (href=\"other-slug\") do not resolve — link with \
         an explicit .html. Build only spaces you trust; serve the output at a web root \
         (or open index.html) so the base libs resolve."
            .to_string(),
    ];
    if slugs.is_empty() {
        warnings
            .push("the space contains no artifacts: the build produced no entry page.".to_string());
    }

    // Validate the output directory (read-only: metadata + read_dir). Done for BOTH
    // dry-run and the real run so --dry-run performs the same non-mutating checks
    // the real run does (§11). Pass --force to preview/allow a non-empty target.
    guard_out_dir(&out, force, json);

    if dry_run {
        emit_build_report(
            json,
            true,
            &out,
            &name,
            mode,
            &slugs,
            &files,
            home.as_deref(),
            index.as_deref(),
            &warnings,
        );
        return;
    }

    if let Err(e) = build::write_files(&out, &files) {
        exit_error(
            json,
            2,
            "io_error",
            &format!("cannot write build output under {}: {e}", out.display()),
            None,
            None,
        );
    }

    // Prefer the canonical absolute path now that the directory exists.
    let resolved = std::fs::canonicalize(&out).unwrap_or_else(|_| out.clone());
    emit_build_report(
        json,
        false,
        &resolved,
        &name,
        mode,
        &slugs,
        &files,
        home.as_deref(),
        index.as_deref(),
        &warnings,
    );
}

/// Reject an output directory that equals or is nested inside the source space.
/// Both are resolved to absolute paths (`out` via its nearest existing ancestor,
/// since it need not exist yet) so the check is robust to `.`/`..`/symlink
/// spellings. This is independent of `--force`: writing at/into the source would
/// overwrite artifacts or seed the next scan with generated output.
fn guard_out_not_in_space(space_dir: &Path, out: &Path, json: bool) {
    let space_abs = std::fs::canonicalize(space_dir).unwrap_or_else(|_| space_dir.to_path_buf());
    let out_abs = abs_via_nearest_ancestor(out);
    if out_abs == space_abs || out_abs.starts_with(&space_abs) {
        exit_error(
            json,
            1,
            "output_inside_space",
            &format!(
                "output {} is the source space {} (or nested inside it); choose an output \
                 directory outside the space so the build cannot overwrite or re-scan its own output",
                out.display(),
                space_dir.display()
            ),
            Some(&out.display().to_string()),
            None,
        );
    }
}

/// Resolve `path` to an absolute path even when it does not exist yet:
/// canonicalize the deepest existing ancestor and re-append the non-existent tail.
/// Used to compare a not-yet-created output dir against the (existing) source root.
fn abs_via_nearest_ancestor(path: &Path) -> PathBuf {
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = path.to_path_buf();
    loop {
        if let Ok(canon) = std::fs::canonicalize(&cur) {
            let mut result = canon;
            for seg in tail.iter().rev() {
                result.push(seg);
            }
            return result;
        }
        match cur.file_name() {
            Some(name) => tail.push(name.to_os_string()),
            None => break,
        }
        if !cur.pop() {
            break;
        }
    }
    // Nothing on the path existed (or a rootless relative path): best-effort absolute.
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

/// Refuse to write into an existing non-empty `<out>` unless `--force`. A path that
/// does not exist is fine (`write_files` creates it); a path that exists but is not
/// a directory is always an error. IO errors reading the directory are system
/// errors (exit 2).
fn guard_out_dir(out: &Path, force: bool, json: bool) {
    match std::fs::metadata(out) {
        Ok(m) if m.is_dir() => {
            if !force {
                // A read failure here is a SYSTEM error (exit 2), not "empty"; a
                // first entry that errors is likewise an I/O failure, not "non-empty".
                let mut it = match std::fs::read_dir(out) {
                    Ok(it) => it,
                    Err(e) => exit_error(
                        json,
                        2,
                        "io_error",
                        &format!("cannot read output directory {}: {e}", out.display()),
                        None,
                        None,
                    ),
                };
                match it.next() {
                    None => {} // empty → fine
                    Some(Err(e)) => exit_error(
                        json,
                        2,
                        "io_error",
                        &format!("cannot read output directory {}: {e}", out.display()),
                        None,
                        None,
                    ),
                    Some(Ok(_)) => exit_error(
                        json,
                        1,
                        "output_not_empty",
                        &format!(
                            "output directory {} is not empty; pass --force to write into it \
                             (existing files may be overwritten)",
                            out.display()
                        ),
                        Some(&out.display().to_string()),
                        None,
                    ),
                }
            }
        }
        Ok(_) => exit_error(
            json,
            1,
            "output_not_a_directory",
            &format!(
                "{} exists and is not a directory; give a directory path for the build output",
                out.display()
            ),
            Some(&out.display().to_string()),
            None,
        ),
        // Absent: created by `write_files`.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => exit_error(
            json,
            2,
            "io_error",
            &format!("cannot access {}: {e}", out.display()),
            None,
            None,
        ),
    }
}

/// Emit the `build` result (`dry` = false) or dry-run plan (`dry` = true). The
/// dry-run form carries the AI-first §11 `would[]` planning list and `dry_run:
/// true`; the real-run form reports `built: true` and the written counts. Both
/// share the descriptive fields so a caller reads the same shape either way.
#[allow(clippy::too_many_arguments)]
fn emit_build_report(
    json: bool,
    dry: bool,
    out: &Path,
    name: &str,
    mode: LibMode,
    slugs: &[String],
    files: &[build::OutFile],
    home: Option<&str>,
    index: Option<&str>,
    warnings: &[String],
) {
    if json {
        // `built`/`dry_run` are both present in every payload (one true, one false)
        // so an AI consumer reads a stable shape without mode-dependent field probing.
        let mut payload = json!({
            "schema_version": SCHEMA_VERSION,
            "built": !dry,
            "dry_run": dry,
            "space": name,
            "out": out.display().to_string(),
            "mode": mode.as_str(),
            "home": home,
            "index": index,
            "artifacts": slugs,
            "pages": slugs.len(),
            "files": files.len(),
            "base_libs_bundled": mode == LibMode::SelfContained,
            "warnings": warnings,
        });
        if dry {
            let would: Vec<serde_json::Value> = files
                .iter()
                .map(|f| {
                    json!({
                        "action": "write",
                        "resource": "file",
                        "path": f.rel_path,
                        "bytes": f.bytes.len(),
                    })
                })
                .collect();
            payload
                .as_object_mut()
                .expect("object literal")
                .insert("would".into(), json!(would));
        }
        emit_json_line(&payload);
    } else if dry {
        for w in warnings {
            eprintln!("warning: {w}");
        }
        eprintln!(
            "glasspad build (dry run): would write {} file(s) for space '{name}' ({}) to {}",
            files.len(),
            mode.as_str(),
            out.display()
        );
        for f in files {
            eprintln!("  {} ({} bytes)", f.rel_path, f.bytes.len());
        }
    } else {
        for w in warnings {
            eprintln!("warning: {w}");
        }
        // Bare output path on stdout (composable); human summary on stderr.
        println!("{}", out.display());
        eprintln!(
            "glasspad built space '{name}' ({}) to {} — {} page(s), {} file(s)",
            mode.as_str(),
            out.display(),
            slugs.len(),
            files.len()
        );
    }
}

// --- open -----------------------------------------------------------------

/// `glasspad open <space> [--port] [--no-browser]` — resolve a served space's URL
/// and open it in the browser. Pure and composable: it builds the URL from the
/// space name + port and launches the OS opener; it holds no state and does not
/// probe whether a server is actually up (that is the caller's `serve`/`create`).
pub fn open(space: String, port: u16, json: bool, no_browser: bool) {
    if !artifact_host::valid_space(&space) {
        exit_error(
            json,
            1,
            "invalid_space_name",
            &format!(
                "invalid space {space:?}: a space name must be lowercase [a-z0-9-], \
                 start alphanumeric, be ≤64 chars, and not be reserved ({})",
                artifact_host::RESERVED.join(", ")
            ),
            Some(&space),
            None, // see load_single_file: reserved names are a deny list, not `expected`
        );
    }
    let url = format!("http://127.0.0.1:{port}/{space}/");
    let launched = if no_browser {
        false
    } else {
        launch_browser(&url)
    };

    // A requested-but-failed launch must not look like a deliberate `--no-browser`:
    // surface it as a non-fatal warning (§4/§10) so the caller can tell them apart.
    // Exit stays 0 — the URL is still valid and printed for the caller to use.
    let mut warnings: Vec<String> = Vec::new();
    if !no_browser && !launched {
        warnings.push(
            "browser launch failed (no opener available or spawn failed); \
             the URL is still valid — open it manually"
                .to_string(),
        );
    }

    if json {
        let payload = json!({
            "schema_version": SCHEMA_VERSION,
            "space": space,
            "port": port,
            "url": url,
            "browser_launched": launched,
            "warnings": warnings,
        });
        emit_json_line(&payload);
    } else {
        for w in &warnings {
            eprintln!("warning: {w}");
        }
        if launched {
            println!("Opening {url}");
        } else {
            // Pipe-friendly: the bare URL on stdout so `open --no-browser` composes.
            println!("{url}");
        }
    }
}

/// Launch the OS browser opener. Returns whether the opener was spawned (not
/// whether a browser actually appeared — the child is fire-and-forget).
fn launch_browser(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(target_os = "linux")]
    let cmd = "xdg-open";
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = url;
        return false;
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        std::process::Command::new(cmd).arg(url).spawn().is_ok()
    }
}

// --- host-serve (hosted share server) -------------------------------------

/// `glasspad host-serve --bind <ip:port> --public-host <origin> --api-key-file
/// <path> --store <dir> [--retention-days <n>]` — run the long-lived hosted share
/// server (0.3.0): API-key-authenticated ingest + unguessable capability-slug
/// public read. A *separate run mode* from loopback `serve` — it binds the given
/// public address and never uses the loopback DNS-rebinding guard (see
/// `hosted` module docs / `plan.md` §8). Fail-fast + fail-closed: a bad origin, an
/// unreadable/empty/malformed key file, or an un-openable store each exit with an
/// informative envelope *before* the server binds.
pub async fn host_serve(
    bind: SocketAddr,
    public_host: String,
    api_key_file: PathBuf,
    store: PathBuf,
    retention_days: i64,
    json: bool,
) {
    // Validate the public origin (AI-first §1 fail-fast) before any I/O.
    let public_origin = match hosted::validate_public_origin(&public_host) {
        Ok(o) => o,
        Err(msg) => exit_error(
            json,
            1,
            "invalid_public_host",
            &msg,
            Some(&public_host),
            None,
        ),
    };

    // Load the operator key file — fail-closed: the server never comes up with an
    // ingest surface no key (or any key) can authenticate.
    let keys = match KeyTable::load(&api_key_file) {
        Ok(k) => Arc::new(k),
        Err(e) => {
            let (code, exit) = match e {
                KeyFileError::Io(_) => ("api_key_file_unreadable", 2),
                _ => ("invalid_api_key_file", 1),
            };
            exit_error(
                json,
                exit,
                code,
                &e.to_string(),
                Some(&api_key_file.display().to_string()),
                None,
            );
        }
    };
    let key_count = keys.len();

    let config = HostedConfig {
        bind,
        public_origin: public_origin.clone(),
        store_root: store,
        retention_days,
    };

    let handle = match hosted::run(config, keys).await {
        Ok(h) => h,
        Err(msg) => exit_error(json, 2, "host_start_failed", &msg, None, None),
    };
    let local = handle
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| bind.to_string());
    emit_host_serving(
        json,
        &local,
        &public_origin,
        handle.pages,
        key_count,
        retention_days,
    );

    if let Err(e) = handle.serve().await {
        exit_error(
            json,
            2,
            "serve_failed",
            &format!("hosted server stopped with an error: {e}"),
            None,
            None,
        );
    }
}

/// Startup envelope for `host-serve` (mirrors [`emit_serving`]): a long-running
/// announcement, not a terminal result. `--json` → stdout; text → stderr.
fn emit_host_serving(
    json: bool,
    bind: &str,
    public_origin: &str,
    pages: usize,
    keys: usize,
    retention_days: i64,
) {
    if json {
        let payload = json!({
            "schema_version": SCHEMA_VERSION,
            "serving": true,
            "mode": "hosted",
            "bind": bind,
            "public_host": public_origin,
            "ingest": format!("{public_origin}/api/v1/pages"),
            "mount": hosted::MOUNT,
            "pages": pages,
            "api_keys": keys,
            "retention_days": retention_days,
            "warnings": [],
        });
        emit_json_line(&payload);
    } else {
        eprintln!(
            "glasspad hosted share server on {bind} (public {public_origin}); \
             {pages} page(s), {keys} key(s), {retention_days}d retention"
        );
    }
}

// --- publish (client) -----------------------------------------------------

/// Config the `publish` client reads (lowest precedence; flag > env > file). YAML
/// at `${XDG_CONFIG_HOME:-~/.config}/glasspad/config.yaml` on every platform. For
/// backward compatibility an existing file at the platform `dirs::config_dir()`
/// location (macOS `~/Library/Application Support/glasspad/config.yaml`) is still
/// read as a fallback when no XDG-path file exists. Both fields optional.
#[derive(Default, serde::Deserialize)]
struct PublishConfig {
    server: Option<String>,
    api_key: Option<String>,
}

/// `glasspad publish <file> [--server <url>] [--api-key <key>] [--markdown
/// [--template <ref>]] [--title <t>] [--no-open]` — publish one page to a hosted
/// share server and print `{slug, url}`. Config precedence (AI-first §8):
/// flag > `$GLASSPAD_SERVER`/`$GLASSPAD_API_KEY` > config file.
///
/// The API key is never printed to stdout/stderr by this command. Note, however,
/// that a key passed via `--api-key` is visible in process listings + shell
/// history on shared machines — prefer `$GLASSPAD_API_KEY` or the config file for
/// anything but throwaway use.
#[allow(clippy::too_many_arguments)]
pub async fn publish(
    file: PathBuf,
    server: Option<String>,
    api_key: Option<String>,
    markdown: bool,
    template: Option<String>,
    title: Option<String>,
    idempotency_key: Option<String>,
    json: bool,
    no_open: bool,
) {
    let cfg = load_publish_config(json);

    let server = resolve_setting(server, "GLASSPAD_SERVER", cfg.server).unwrap_or_else(|| {
        exit_error(
            json,
            1,
            "missing_server",
            "no hosted server URL: pass --server <url>, set $GLASSPAD_SERVER, or add `server:` \
             to ~/.config/glasspad/config.yaml",
            None,
            None,
        )
    });
    let api_key = resolve_setting(api_key, "GLASSPAD_API_KEY", cfg.api_key).unwrap_or_else(|| {
        exit_error(
            json,
            1,
            "missing_api_key",
            "no API key: pass --api-key <key>, set $GLASSPAD_API_KEY, or add `api_key:` to \
             ~/.config/glasspad/config.yaml",
            None,
            None,
        )
    });

    // Read the source file (bounded, UTF-8) — the same strict checks `create` uses.
    let noun = if markdown { "markdown" } else { "html" };
    let content = read_capped_utf8_file(&file, noun, "no_such_path", json);

    // Build the ingest JSON body.
    let mut body = serde_json::Map::new();
    if markdown {
        body.insert("markdown".into(), json!(content));
        if let Some(t) = &template {
            // A built-in name is sent as-is; anything else is a template FILE path,
            // read + sent as an inline template (matches `render`'s resolution).
            let resolved = resolve_publish_template(t, json);
            body.insert("template".into(), json!(resolved));
        }
    } else {
        if template.is_some() {
            exit_error(
                json,
                1,
                "template_without_markdown",
                "--template only applies with --markdown (raw HTML is published verbatim)",
                None,
                None,
            );
        }
        body.insert("html".into(), json!(content));
    }
    if let Some(t) = &title {
        body.insert("title".into(), json!(t));
    }
    // An idempotency key is passed through verbatim; the server validates length
    // and non-emptiness and enforces the exactly-once semantics.
    if let Some(k) = resolve_setting(idempotency_key, "GLASSPAD_IDEMPOTENCY_KEY", None) {
        body.insert("idempotency_key".into(), json!(k));
    }

    // Warn (non-fatal) if the bearer key would cross a plaintext connection to a
    // non-loopback host — it can be sniffed/replayed on a public network.
    if server.starts_with("http://") && !server_is_loopback(&server) {
        eprintln!(
            "warning: publishing over plaintext http:// to a non-local host sends the API key \
             in the clear; prefer https://"
        );
    }

    let url = format!("{}/api/v1/pages", server.trim_end_matches('/'));
    // Disable redirects (never replay the bearer to a redirected target) and set
    // timeouts so a hung/hostile server cannot stall the client indefinitely.
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(60))
        .build()
    {
        Ok(c) => c,
        Err(e) => exit_error(json, 2, "client_init_failed", &e.to_string(), None, None),
    };
    let resp = client
        .post(&url)
        .bearer_auth(&api_key)
        .json(&serde_json::Value::Object(body))
        .send()
        .await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) => exit_error(
            json,
            2,
            "request_failed",
            // reqwest's Display never includes the bearer token; safe to surface.
            &format!("cannot reach {url}: {e}"),
            None,
            None,
        ),
    };

    let status = resp.status();
    let payload: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        let msg = payload
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("the server rejected the publish");
        let code = payload
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(|c| c.as_str())
            .unwrap_or("publish_rejected");
        // 4xx is a caller/request error (fixable → 1); 3xx (redirects are disabled,
        // so a 3xx is an unexpected server contract) and 5xx are system errors (2).
        let exit = if status.is_client_error() { 1 } else { 2 };
        exit_error(
            json,
            exit,
            code,
            &format!("{msg} (HTTP {})", status.as_u16()),
            None,
            None,
        );
    }

    // A 2xx with a missing/empty slug or URL is a broken server contract, not a
    // success — surface it rather than printing an empty "published '' to".
    let slug = payload
        .get("slug")
        .and_then(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let page_url = payload
        .get("url")
        .and_then(|u| u.as_str())
        .filter(|u| !u.is_empty())
        .map(str::to_string);
    let (slug, page_url) = match (slug, page_url) {
        (Some(s), Some(u)) => (s, u),
        _ => exit_error(
            json,
            2,
            "malformed_response",
            &format!(
                "server returned {} but no slug/url in the body",
                status.as_u16()
            ),
            None,
            None,
        ),
    };

    let launched = if no_open {
        false
    } else {
        launch_browser(&page_url)
    };

    if json {
        let out = json!({
            "schema_version": SCHEMA_VERSION,
            "published": true,
            "slug": slug,
            "url": page_url,
            "browser_launched": launched,
            "warnings": [],
        });
        emit_json_line(&out);
    } else {
        // Bare URL on stdout (composable); a human note on stderr.
        println!("{page_url}");
        eprintln!("published '{slug}' to {page_url}");
    }
}

/// `glasspad push-round <slug> <file> [--server <url>] [--api-key <key>] [--markdown
/// [--template <ref>]]` — the B2 **multi-round** client. Re-render an already-published
/// hosted page in response to a submission: it POSTs the new body to
/// `/api/v1/pages/<slug>/rounds` (API-key auth, owner-scoped) and the server swaps the
/// live page's content in place for every connected viewer, then prints
/// `{slug, round, content_version}`. Config precedence mirrors `publish`
/// (flag > `$GLASSPAD_SERVER`/`$GLASSPAD_API_KEY` > config file). The new
/// `content_version` is the value the next submission for this round will echo.
pub async fn push_round(
    slug: String,
    file: PathBuf,
    server: Option<String>,
    api_key: Option<String>,
    markdown: bool,
    template: Option<String>,
    json: bool,
) {
    let cfg = load_publish_config(json);
    let server = resolve_setting(server, "GLASSPAD_SERVER", cfg.server).unwrap_or_else(|| {
        exit_error(
            json,
            1,
            "missing_server",
            "no hosted server URL: pass --server <url>, set $GLASSPAD_SERVER, or add `server:` \
             to ~/.config/glasspad/config.yaml",
            None,
            None,
        )
    });
    let api_key = resolve_setting(api_key, "GLASSPAD_API_KEY", cfg.api_key).unwrap_or_else(|| {
        exit_error(
            json,
            1,
            "missing_api_key",
            "no API key: pass --api-key <key>, set $GLASSPAD_API_KEY, or add `api_key:` to \
             ~/.config/glasspad/config.yaml",
            None,
            None,
        )
    });

    // Read the new round source (bounded, UTF-8) — the same strict checks `publish` uses.
    let noun = if markdown { "markdown" } else { "html" };
    let content = read_capped_utf8_file(&file, noun, "no_such_path", json);

    let mut body = serde_json::Map::new();
    if markdown {
        body.insert("markdown".into(), json!(content));
        if let Some(t) = &template {
            let resolved = resolve_publish_template(t, json);
            body.insert("template".into(), json!(resolved));
        }
    } else {
        if template.is_some() {
            exit_error(
                json,
                1,
                "template_without_markdown",
                "--template only applies with --markdown (raw HTML is pushed verbatim)",
                None,
                None,
            );
        }
        body.insert("html".into(), json!(content));
    }

    if server.starts_with("http://") && !server_is_loopback(&server) {
        eprintln!(
            "warning: pushing a round over plaintext http:// to a non-local host sends the API \
             key in the clear; prefer https://"
        );
    }

    let url = format!(
        "{}/api/v1/pages/{}/rounds",
        server.trim_end_matches('/'),
        slug
    );
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(60))
        .build()
    {
        Ok(c) => c,
        Err(e) => exit_error(json, 2, "client_init_failed", &e.to_string(), None, None),
    };
    let resp = client
        .post(&url)
        .bearer_auth(&api_key)
        .json(&serde_json::Value::Object(body))
        .send()
        .await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) => exit_error(
            json,
            2,
            "request_failed",
            &format!("cannot reach {url}: {e}"),
            None,
            None,
        ),
    };

    let status = resp.status();
    let payload: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        let msg = payload
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("the server rejected the round push");
        let code = payload
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(|c| c.as_str())
            .unwrap_or("push_round_rejected");
        let exit = if status.is_client_error() { 1 } else { 2 };
        exit_error(
            json,
            exit,
            code,
            &format!("{msg} (HTTP {})", status.as_u16()),
            None,
            None,
        );
    }

    let round = payload.get("round").and_then(|r| r.as_u64());
    let content_version = payload
        .get("content_version")
        .and_then(|c| c.as_str())
        .filter(|c| !c.is_empty())
        .map(str::to_string);
    let (round, content_version) = match (round, content_version) {
        (Some(r), Some(cv)) => (r, cv),
        _ => exit_error(
            json,
            2,
            "malformed_response",
            &format!(
                "server returned {} but no round/content_version in the body",
                status.as_u16()
            ),
            None,
            None,
        ),
    };

    if json {
        let out = json!({
            "schema_version": SCHEMA_VERSION,
            "pushed": true,
            "slug": slug,
            "round": round,
            "content_version": content_version,
            "warnings": [],
        });
        emit_json_line(&out);
    } else {
        eprintln!("pushed round {round} of '{slug}' (content_version {content_version})");
    }
}

/// Resolve one setting by precedence: explicit flag > environment variable > config
/// file value. An empty/whitespace flag or env value is treated as unset (AI-first
/// §1 — no silent empties). Returns `None` if unset at every level.
fn resolve_setting(flag: Option<String>, env: &str, file: Option<String>) -> Option<String> {
    let nonempty = |s: String| {
        let t = s.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
    };
    flag.and_then(nonempty)
        .or_else(|| std::env::var(env).ok().and_then(nonempty))
        .or_else(|| file.and_then(nonempty))
}

/// Best-effort: is the `--server` URL a loopback host (where plaintext http is
/// acceptable)? Used only to decide whether to warn about a cleartext bearer.
fn server_is_loopback(server: &str) -> bool {
    let authority = server
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(server);
    let host = authority
        .split(['/', ':'])
        .next()
        .unwrap_or(authority)
        .to_ascii_lowercase();
    host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "[::1]"
}

/// Candidate config-file paths in precedence order: the documented XDG path
/// (`$XDG_CONFIG_HOME`, else `~/.config`) first on every platform, then — for
/// backward compatibility — the platform `dirs::config_dir()` location (on macOS
/// `~/Library/Application Support`), which older installs may still use. The
/// first candidate that exists wins.
fn publish_config_candidates() -> Vec<PathBuf> {
    publish_config_candidates_from(
        // Per the XDG spec an empty value is treated as unset (falls back to ~/.config).
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from),
        dirs::home_dir(),
        dirs::config_dir(),
    )
}

/// Pure candidate-ordering logic (env/home/config-dir passed in so it is testable).
fn publish_config_candidates_from(
    xdg_config_home: Option<PathBuf>,
    home_dir: Option<PathBuf>,
    platform_config_dir: Option<PathBuf>,
) -> Vec<PathBuf> {
    let leaf = |base: PathBuf| base.join("glasspad").join("config.yaml");
    let mut candidates = Vec::new();

    // Documented, cross-platform path: $XDG_CONFIG_HOME (if set & absolute), else ~/.config.
    let xdg = xdg_config_home
        .filter(|p| p.is_absolute())
        .or_else(|| home_dir.map(|h| h.join(".config")));
    if let Some(dir) = xdg {
        candidates.push(leaf(dir));
    }

    // Backward-compat fallback: the platform config dir (macOS Application Support).
    // Filter for absoluteness too — on Unix `dirs::config_dir()` echoes a relative
    // `$XDG_CONFIG_HOME` verbatim, and a relative candidate would be read against the
    // process CWD (an unintended file on multi-user/container hosts).
    if let Some(dir) = platform_config_dir.filter(|p| p.is_absolute()) {
        let legacy = leaf(dir);
        if !candidates.contains(&legacy) {
            candidates.push(legacy);
        }
    }

    candidates
}

/// Load the optional publish config file. A candidate that is simply absent
/// (`NotFound`) is skipped so resolution advances to the next; a candidate that
/// *exists* but cannot be read (permissions, a directory, non-UTF-8, …) or is
/// malformed is a user error surfaced informatively — never silently swallowed
/// into the legacy fallback, which could substitute a different server/api_key.
/// Returns the config from the first candidate that exists.
fn load_publish_config(json: bool) -> PublishConfig {
    for path in publish_config_candidates() {
        let contents = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            // Genuinely absent → try the next candidate.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            // Exists but unreadable → do not fall through to a different config.
            Err(e) => exit_error(
                json,
                1,
                "unreadable_config",
                &format!("cannot read {}: {e}", path.display()),
                None,
                None,
            ),
        };
        return match serde_yaml::from_str::<PublishConfig>(&contents) {
            Ok(c) => c,
            Err(e) => exit_error(
                json,
                1,
                "invalid_config",
                &format!("malformed {}: {e}", path.display()),
                None,
                None,
            ),
        };
    }
    PublishConfig::default() // no config at any candidate path
}

/// Resolve the `--template` reference for `publish`: a built-in name is sent
/// verbatim; anything else is a path to a template file, read + returned as an
/// inline template string. Mirrors `resolve_template`'s built-in-vs-path rule.
fn resolve_publish_template(reference: &str, json: bool) -> String {
    if render::builtin_template(reference).is_some() {
        return reference.to_string();
    }
    // A path to a template file (read strictly, bounded, UTF-8).
    read_capped_utf8_file(Path::new(reference), "template", "template_not_found", json)
}

// --- await-submission (return-channel client) -----------------------------

/// `glasspad await-submission <slug> [--since <cursor>] [--timeout <secs>]
/// [--server <url>] [--api-key <key>] [--port <port>] [--json]` — block on the
/// next user submission an interactive artifact sent back, then print it.
///
/// This is the **primary agent-facing surface** of the return channel (design
/// A3): the agent runs it **backgrounded** and gets the human's answer as the
/// command's return value — no polling loop, no cursor bookkeeping. It rides a
/// **server-side long-poll** (`…/submissions/wait`), so it wastes no requests
/// while nothing arrives, and it always returns within `--timeout` with a
/// **distinct** "timed-out, no submission" result (exit code 3) so a backgrounded
/// caller can re-arm from the returned `cursor` or give up.
///
/// Mode selection: an explicit `--server` selects the **hosted** server (API-key
/// auth, `<slug>` = the page slug); an explicit `--port` (a loopback-only concept)
/// selects the **loopback** `serve` process even when a hosted server is configured
/// (`<slug>` = the space name, no auth — loopback only); with neither flag it uses
/// `$GLASSPAD_SERVER`/config if set, else loopback on the default port.
#[allow(clippy::too_many_arguments)]
pub async fn await_submission(
    slug: String,
    since: u64,
    timeout: u64,
    server: Option<String>,
    api_key: Option<String>,
    port: Option<u16>,
    stream: bool,
    follow: bool,
    json: bool,
) {
    // The slug/space addressing token obeys the same grammar the router enforces.
    if !artifact_host::valid_space(&slug) {
        exit_error(
            json,
            1,
            "invalid_slug",
            "slug must be lowercase [a-z0-9-], start alphanumeric, ≤64 chars, and not be reserved",
            Some(&slug),
            None,
        );
    }
    let timeout = timeout.clamp(1, crate::submissions::MAX_WAIT_SECS);

    let cfg = load_publish_config(json);
    // Mode selection: an explicit `--server` forces hosted; an explicit `--port`
    // (a loopback-only concept) forces loopback even when a hosted server is
    // configured; otherwise fall back to the configured/env server, else loopback.
    let server_flag = server.filter(|s| !s.trim().is_empty());
    let server = match (server_flag, port) {
        (Some(s), _) => Some(s),
        (None, Some(_)) => None,
        (None, None) => resolve_setting(None, "GLASSPAD_SERVER", cfg.server),
    };

    // Build the wait URL + optional bearer per mode.
    let (url, bearer) = match server {
        Some(server) => {
            let api_key =
                resolve_setting(api_key, "GLASSPAD_API_KEY", cfg.api_key).unwrap_or_else(|| {
                    exit_error(
                        json,
                        1,
                        "missing_api_key",
                        "no API key: pass --api-key <key>, set $GLASSPAD_API_KEY, or add `api_key:` \
                         to ~/.config/glasspad/config.yaml (a hosted --server requires a key)",
                        None,
                        None,
                    )
                });
            if server.starts_with("http://") && !server_is_loopback(&server) {
                eprintln!(
                    "warning: awaiting over plaintext http:// to a non-local host sends the API \
                     key in the clear; prefer https://"
                );
            }
            let base = server.trim_end_matches('/');
            let url = if stream {
                format!("{base}/api/v1/pages/{slug}/submissions/stream?since={since}")
            } else {
                format!(
                    "{base}/api/v1/pages/{slug}/submissions/wait?since={since}&timeout={timeout}"
                )
            };
            (url, Some(api_key))
        }
        None => {
            // Loopback: target the local `serve` process on the resolved port.
            let port = resolve_port(port, json);
            let url = if stream {
                format!("http://127.0.0.1:{port}/{slug}/_gp/submissions/stream?since={since}")
            } else {
                format!(
                    "http://127.0.0.1:{port}/{slug}/_gp/submissions/wait?since={since}&timeout={timeout}"
                )
            };
            (url, None)
        }
    };

    // The HTTP timeout must outlast the server-side long-poll so the *server*
    // returns the "timed out" result first (rather than the client aborting).
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(timeout + 15))
        .build()
    {
        Ok(c) => c,
        Err(e) => exit_error(json, 2, "client_init_failed", &e.to_string(), None, None),
    };
    let mut request = client.get(&url);
    if let Some(k) = &bearer {
        request = request.bearer_auth(k);
    }
    // SSE requests advertise the media type so a proxy never buffers/transcodes.
    if stream {
        request = request.header(reqwest::header::ACCEPT, "text/event-stream");
    }
    let resp = match request.send().await {
        Ok(r) => r,
        Err(e) => exit_error(
            json,
            2,
            "request_failed",
            &format!("cannot reach {url}: {e}"),
            None,
            None,
        ),
    };

    // SSE transport (A2): consume the server-push stream instead of the long-poll.
    if stream {
        consume_submission_stream(resp, since, timeout, follow, json).await;
    }

    let status = resp.status();
    let payload: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        let msg = payload
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("the server rejected the wait");
        let code = payload
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(|c| c.as_str())
            .unwrap_or("await_rejected");
        let exit = if status.is_client_error() { 1 } else { 2 };
        exit_error(
            json,
            exit,
            code,
            &format!("{msg} (HTTP {})", status.as_u16()),
            None,
            None,
        );
    }

    let submissions = payload
        .get("submissions")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();
    let cursor = payload
        .get("cursor")
        .and_then(|c| c.as_u64())
        .unwrap_or(since);
    let timed_out = payload
        .get("timed_out")
        .and_then(|t| t.as_bool())
        .unwrap_or(submissions.is_empty());

    if json {
        emit_json_line(&json!({
            "schema_version": SCHEMA_VERSION,
            "timed_out": timed_out,
            "submissions": submissions,
            "cursor": cursor,
            "warnings": [],
        }));
    } else if timed_out {
        eprintln!("no submission before the {timeout}s timeout (re-arm from cursor {cursor})");
    } else {
        // stdout is the data channel: one compact JSON submission per line, so a
        // backgrounded caller can read the answer directly.
        for s in &submissions {
            println!("{}", serde_json::to_string(s).unwrap_or_default());
        }
        eprintln!(
            "received {} submission(s) (next cursor {cursor})",
            submissions.len()
        );
    }
    // Exit code encodes the outcome: 0 = at least one submission, 3 = timed out with
    // none (a distinct, non-error status so a backgrounded agent can branch on it).
    std::process::exit(if timed_out { 3 } else { 0 });
}

/// Consume the return-channel **SSE stream** (A2 transport) and diverge with the same
/// exit-code contract as the long-poll path: `0` once at least one submission is
/// printed, `3` if the `timeout` elapses (or the server closes the stream) with none.
///
/// Each `submission` event's `data` is a submission's public JSON; under `--json` it is
/// re-emitted as the same `{submissions:[…], cursor, timed_out}` envelope the long-poll
/// prints (one per event), otherwise as one compact JSON line on stdout (the data
/// channel). Without `--follow` the first submission ends the command (backgrounded
/// ergonomics — fire, get the answer as the result); with `--follow` it keeps printing
/// each as it lands until `timeout`. The cursor comes from each record's own `id`, and
/// only **strictly-forward** ids are accepted (a duplicate/backward/id-less event is
/// ignored) so the client keeps the same no-redeliver contract as the store.
async fn consume_submission_stream(
    resp: reqwest::Response,
    since: u64,
    timeout: u64,
    follow: bool,
    json: bool,
) -> ! {
    use tokio_stream::StreamExt as _;
    let status = resp.status();
    if !status.is_success() {
        let payload: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
        let msg = payload
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("the server rejected the stream");
        let code = payload
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(|c| c.as_str())
            .unwrap_or("stream_rejected");
        let exit = if status.is_client_error() { 1 } else { 2 };
        exit_error(
            json,
            exit,
            code,
            &format!("{msg} (HTTP {})", status.as_u16()),
            None,
            None,
        );
    }
    // A 2xx that is not an SSE stream (a proxy / login page returning 200 text/html)
    // must be a clear protocol error, not a silent "timed out with no submission".
    let is_event_stream = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|ct| {
            ct.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("text/event-stream")
        })
        .unwrap_or(false);
    if !is_event_stream {
        exit_error(
            json,
            2,
            "unexpected_content_type",
            "the server did not return an SSE stream (Content-Type text/event-stream)",
            None,
            None,
        );
    }

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout);
    let mut body = resp.bytes_stream();
    let mut decoder = SseDecoder::default();
    let mut cursor = since;
    let mut received = 0usize;

    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        let chunk = match tokio::time::timeout(deadline - now, body.next()).await {
            Err(_) => break,      // our logical --timeout elapsed
            Ok(None) => break,    // the server closed the stream
            Ok(Some(Ok(b))) => b, // more stream bytes
            Ok(Some(Err(e))) => {
                // A mid-hold transport error: if we already delivered something this is
                // a normal end; otherwise it is a genuine failure.
                if received > 0 {
                    break;
                }
                exit_error(
                    json,
                    2,
                    "stream_failed",
                    &format!("stream error: {e}"),
                    None,
                    None,
                );
            }
        };
        // Decode over raw bytes (never per-chunk lossy UTF-8) with bounded buffers.
        let mut items = Vec::new();
        if decoder.feed(&chunk, &mut items).is_err() {
            exit_error(
                json,
                2,
                "stream_too_large",
                "the SSE stream exceeded the per-line / per-event size bound",
                None,
                None,
            );
        }
        for SseItem::Submission { id, value } in items {
            // Cursor invariant: accept only a strictly-forward id (skip a duplicate,
            // out-of-order, or id-less/degraded event without counting or printing it).
            if id <= cursor {
                continue;
            }
            cursor = id;
            received += 1;
            if json {
                emit_json_line(&json!({
                    "schema_version": SCHEMA_VERSION,
                    "timed_out": false,
                    "submissions": [value],
                    "cursor": cursor,
                    "warnings": [],
                }));
            } else {
                println!("{}", serde_json::to_string(&value).unwrap_or_default());
            }
            if !follow {
                eprintln!("received 1 submission via stream (next cursor {cursor})");
                std::process::exit(0);
            }
        }
    }

    // The stream ended (timeout or server close). Any delivered submissions → success.
    if received > 0 {
        eprintln!("streamed {received} submission(s) (next cursor {cursor})");
        std::process::exit(0);
    }
    if json {
        emit_json_line(&json!({
            "schema_version": SCHEMA_VERSION,
            "timed_out": true,
            "submissions": [],
            "cursor": cursor,
            "warnings": [],
        }));
    } else {
        eprintln!("no submission before the {timeout}s timeout (re-arm from cursor {cursor})");
    }
    std::process::exit(3);
}

/// Upper bound on one buffered SSE line before the peer is treated as hostile: a
/// submission's public JSON (one `data:` line) plus SSE/envelope slack.
const MAX_SSE_LINE_BYTES: usize = crate::submissions::MAX_SUBMISSION_BYTES + 16 * 1024;
/// Upper bound on one event's accumulated `data` across its `data:` lines.
const MAX_SSE_EVENT_BYTES: usize = crate::submissions::MAX_SUBMISSION_BYTES + 32 * 1024;

/// One decoded item the SSE stream produced.
enum SseItem {
    /// A complete `submission` event whose `data` parsed to an object with a numeric id.
    Submission { id: u64, value: serde_json::Value },
}

/// The peer violated an SSE size bound (a line or event exceeded its cap).
#[derive(Debug)]
struct SseOverflow;

/// A bounded, incremental Server-Sent-Events line decoder. It operates on **raw bytes**
/// so a multi-byte UTF-8 code point split across network chunks is never corrupted (a
/// complete line is UTF-8-decoded once, as a whole — the store's payloads are valid
/// UTF-8). Line and event buffers are size-capped, so a hostile or broken `--server`
/// that streams without a newline, or an oversize event, fails closed with
/// [`SseOverflow`] instead of growing memory without bound. Only `submission` events
/// whose `data` is a JSON object with a numeric `id` are surfaced; `id:`/`retry:`/
/// unknown fields and comment (`:`) keep-alives are ignored (the cursor is the record's
/// own `id`).
#[derive(Default)]
struct SseDecoder {
    /// Bytes of the current (still-incomplete) line.
    line: Vec<u8>,
    /// The in-progress event's `event:` name (last one wins before dispatch).
    event: Option<Vec<u8>>,
    /// The in-progress event's accumulated `data:` bytes.
    data: Vec<u8>,
}

impl SseDecoder {
    /// Feed a chunk of raw bytes; append any completed `submission` events to `out`.
    fn feed(&mut self, chunk: &[u8], out: &mut Vec<SseItem>) -> Result<(), SseOverflow> {
        for &b in chunk {
            if b == b'\n' {
                let mut line = std::mem::take(&mut self.line);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                self.dispatch_line(&line, out)?;
            } else {
                if self.line.len() >= MAX_SSE_LINE_BYTES {
                    return Err(SseOverflow);
                }
                self.line.push(b);
            }
        }
        Ok(())
    }

    /// Process one complete line (SSE field grammar).
    fn dispatch_line(&mut self, line: &[u8], out: &mut Vec<SseItem>) -> Result<(), SseOverflow> {
        if line.is_empty() {
            // Blank line → dispatch the accumulated event, then reset for the next one.
            let is_submission = self.event.as_deref() == Some(b"submission");
            let data = std::mem::take(&mut self.data);
            self.event = None;
            if is_submission
                && let Ok(v) = serde_json::from_slice::<serde_json::Value>(&data)
                && let Some(id) = v.get("id").and_then(|i| i.as_u64())
            {
                out.push(SseItem::Submission { id, value: v });
            }
            return Ok(());
        }
        if line.first() == Some(&b':') {
            return Ok(()); // comment / keep-alive
        }
        // `field: value`; one optional space after the colon is stripped. A line with no
        // colon is a field name with an empty value (per the SSE grammar).
        let (field, mut value) = match line.iter().position(|&b| b == b':') {
            Some(i) => (&line[..i], &line[i + 1..]),
            None => (line, &b""[..]),
        };
        if value.first() == Some(&b' ') {
            value = &value[1..];
        }
        match field {
            b"event" => self.event = Some(value.to_vec()),
            b"data" => {
                // +1 for the joining '\n' between multiple data lines.
                if self.data.len() + value.len() + 1 > MAX_SSE_EVENT_BYTES {
                    return Err(SseOverflow);
                }
                if !self.data.is_empty() {
                    self.data.push(b'\n');
                }
                self.data.extend_from_slice(value);
            }
            // `id:`/`retry:`/unknown fields are ignored — the cursor is the record's id.
            _ => {}
        }
        Ok(())
    }
}

// --- data (legacy-format helper) ------------------------------------------

/// `glasspad data <file> [--format] [--meta]` — parse a legacy CSV/JSON/mbox
/// file into JSON rows on stdout. A standalone convenience over the old data
/// parsers (`glasspad::data`): the section-DSL server that once ingested these
/// formats is gone (Wave 5 / Phase 6), but the parsers remain useful for turning
/// such a file into rows a hand-authored HTML artifact can embed. Never starts a
/// server.
///
/// Output contract (AI-first §10): stdout is the data channel. Under `--json`, a
/// versioned envelope `{schema_version, format, path, row_count, rows[, meta]}`;
/// otherwise the bare rows array (pretty JSON) on stdout with a one-line human
/// summary on stderr. Errors go to stderr via [`exit_error`] with a stable `code`.
pub fn data(file: PathBuf, format: Option<String>, meta: bool, json: bool) {
    use glasspad::data::{infer, limits, types::Dataset};

    // Resolve the format: an explicit `--format` wins, else infer from extension.
    let fmt = match format.as_deref() {
        Some(f) => f.to_string(),
        None => match detect_data_format(&file) {
            Some(f) => f.to_string(),
            None => exit_error(
                json,
                1,
                "unknown_format",
                &format!(
                    "cannot infer a data format from {}: pass --format csv|json|mbox",
                    file.display()
                ),
                Some(&file.display().to_string()),
                Some(vec!["csv".into(), "json".into(), "mbox".into()]),
            ),
        },
    };

    // Read the file, bounded to a 50 MB safety cap. The parsers do not bound by
    // byte count — csv/json/mbox each cap by rows and columns — so this read cap
    // is the only byte bound; the errors below carry the parser's own message.
    let bytes = read_data_file(&file, json);
    // Errors carry a stable `(code, message)` so a UTF-8 failure keeps its own
    // `not_utf8` code instead of collapsing into the generic `parse_failed`.
    let parsed: Result<Dataset, (&'static str, String)> = match fmt.as_str() {
        "csv" => {
            glasspad::data::csv::parse_csv(std::io::Cursor::new(&bytes), limits::MAX_CSV_BYTES)
                .map_err(|e| ("parse_failed", e.to_string()))
        }
        "mbox" => glasspad::data::mbox::parse_mbox_bytes(&bytes)
            .map_err(|e| ("parse_failed", e.to_string())),
        "json" => match std::str::from_utf8(&bytes) {
            Ok(s) => {
                glasspad::data::json::parse_json_str(s).map_err(|e| ("parse_failed", e.to_string()))
            }
            Err(_) => Err((
                "not_utf8",
                format!("{} is not valid UTF-8 (JSON must be UTF-8)", file.display()),
            )),
        },
        // `--format` is a fixed enum and `detect_data_format` only yields these
        // three, so any other value here is a programming error, not user input.
        other => unreachable!("format resolved to csv|json|mbox, got {other:?}"),
    };
    let rows = match parsed {
        Ok(r) => r,
        Err((code, msg)) => exit_error(json, 1, code, &msg, None, None),
    };

    let meta_val = if meta {
        Some(infer::infer_dataset_meta(&rows))
    } else {
        None
    };

    if json {
        // `warnings: []` matches the serve/create/open envelopes so a consumer can
        // read the field unconditionally across commands.
        let mut payload = json!({
            "schema_version": SCHEMA_VERSION,
            "format": fmt,
            "path": file.display().to_string(),
            "row_count": rows.len(),
            "rows": rows,
            "warnings": [],
        });
        if let Some(m) = &meta_val {
            // The envelope is a JSON object literal above, so this always succeeds.
            if let Some(obj) = payload.as_object_mut() {
                obj.insert(
                    "meta".into(),
                    serde_json::to_value(m).unwrap_or(serde_json::Value::Null),
                );
            }
        }
        emit_json_line(&payload);
    } else {
        // Bare rows on stdout (composable); human summary + optional meta on stderr.
        // A serialization failure is a system error, not empty/`[]` output — never
        // pass off a truncated array as the real data.
        let out = match serde_json::to_string_pretty(&rows) {
            Ok(s) => s,
            Err(e) => exit_error(
                json,
                2,
                "serialization_failed",
                &format!("cannot serialize parsed rows: {e}"),
                None,
                None,
            ),
        };
        println!("{out}");
        eprintln!(
            "parsed {} row{} from {} ({fmt})",
            rows.len(),
            if rows.len() == 1 { "" } else { "s" },
            file.display()
        );
        if let Some(m) = &meta_val {
            let fields = m
                .fields
                .iter()
                .map(|(k, v)| format!("{k}:{v:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            eprintln!("fields: {fields}");
        }
    }
}

/// Infer the data format from a file extension: `.csv` → csv, `.json` → json,
/// `.mbox`/`.eml` → mbox. Returns `None` for anything else, so the caller can ask
/// for an explicit `--format`.
fn detect_data_format(file: &Path) -> Option<&'static str> {
    match file
        .extension()
        .and_then(|e| e.to_str())?
        .to_ascii_lowercase()
        .as_str()
    {
        "csv" => Some("csv"),
        "json" => Some("json"),
        "mbox" | "eml" => Some("mbox"),
        _ => None,
    }
}

/// Read a data file into memory, bounded to a 50 MB safety cap. Strict like
/// `create`: a missing path, a directory, a non-regular file, or an oversize
/// file each exits with an informative envelope rather than a silent truncation.
fn read_data_file(file: &Path, json: bool) -> Vec<u8> {
    let meta = match std::fs::metadata(file) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => exit_error(
            json,
            1,
            "no_such_path",
            &format!("no such file: {}", file.display()),
            Some(&file.display().to_string()),
            None,
        ),
        Err(e) => exit_error(
            json,
            2,
            "io_error",
            &format!("cannot read {}: {e}", file.display()),
            None,
            None,
        ),
    };
    if meta.is_dir() {
        exit_error(
            json,
            1,
            "not_a_file",
            &format!(
                "{} is a directory; `data` takes a single file",
                file.display()
            ),
            Some(&file.display().to_string()),
            None,
        );
    }
    // Reject FIFOs / sockets / devices: like `create`, a named pipe reports a
    // zero length (passing the size check) but would then block `open`/read
    // forever. Only a regular file is servable.
    if !meta.is_file() {
        exit_error(
            json,
            1,
            "not_a_file",
            &format!(
                "{} is not a regular file (FIFOs, sockets, and devices are not supported)",
                file.display()
            ),
            Some(&file.display().to_string()),
            None,
        );
    }
    let cap = glasspad::data::limits::MAX_CSV_BYTES as u64;
    if meta.len() > cap {
        exit_error(
            json,
            1,
            "file_too_large",
            &format!(
                "{} is {} bytes, over the {cap}-byte limit",
                file.display(),
                meta.len()
            ),
            None,
            None,
        );
    }
    match read_capped(file, cap) {
        Ok(b) if b.len() as u64 > cap => exit_error(
            json,
            1,
            "file_too_large",
            &format!("{} exceeds the {cap}-byte limit", file.display()),
            None,
            None,
        ),
        Ok(b) => b,
        Err(e) => exit_error(
            json,
            2,
            "io_error",
            &format!("cannot read {}: {e}", file.display()),
            None,
            None,
        ),
    }
}

// --- skill (AI-first §15) -------------------------------------------------

/// `glasspad skill` — print the companion `SKILL.md`, or install it into a Claude
/// Code skills directory (`--install-claude`, `--user` for `~/.claude`).
///
/// Under `--json`, an install emits the AI-first §10 success envelope
/// (`{schema_version, installed, scope, path, created, cli_version}`); the error
/// path (e.g. no project-level `.claude/`) uses the shared [`exit_error`] contract
/// (structured error on stderr, non-zero exit). The bare, non-install `skill`
/// stays a pure content dump on stdout in both modes.
/// Which agent skill directory(ies) `skill --install-claude` writes into.
///
/// The CLI ships one companion skill (`SKILL.md`); the migration from Claude Code
/// to pi.dev means the *same* skill must be discoverable under both harnesses.
/// Claude Code loads `~/.claude/skills/<name>/SKILL.md`; pi.dev loads
/// `~/.pi/agent/skills/<name>/SKILL.md` (and invokes it as `/skill:name`). Rather
/// than force the caller to run the installer twice, `--agent all` (the default)
/// *dual-homes* — one invocation writes both. This mirrors the agent-target
/// convention already used by homebase / orchestratectl (`--agent claude|…|all`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum SkillAgent {
    /// Claude Code skills dir (`~/.claude/skills/` for `--user`, else `./.claude/skills/`).
    Claude,
    /// pi.dev skills dir (`~/.pi/agent/skills/` for `--user`, else `./.pi/skills/`).
    Pi,
    /// Dual-home into both Claude Code and pi.dev (the default).
    All,
}

/// Write `content` to `<dir>/SKILL.md`, creating the tree as needed, and report
/// whether the file was freshly created (vs. overwritten). Any I/O failure exits
/// via the structured-error contract (exit 2), so this never returns on error.
///
/// `created` is decided atomically with an exclusive `create_new` open: it tells a
/// fresh install apart from an in-place refresh even under a racing installer,
/// where a plain `exists()`-then-`write` would misreport. The returned path is
/// canonicalized (the file now exists) for a stable absolute path in the envelope.
fn write_skill_file(dir: &Path, content: &str, json: bool) -> (PathBuf, bool) {
    if let Err(e) = std::fs::create_dir_all(dir) {
        exit_error(
            json,
            2,
            "io_error",
            &format!("cannot create {}: {e}", dir.display()),
            None,
            None,
        );
    }
    let path = dir.join("SKILL.md");
    use std::io::Write as _;
    let created = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut f) => {
            if let Err(e) = f.write_all(content.as_bytes()) {
                exit_error(
                    json,
                    2,
                    "io_error",
                    &format!("cannot write {}: {e}", path.display()),
                    None,
                    None,
                );
            }
            true
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if let Err(e) = std::fs::write(&path, content) {
                exit_error(
                    json,
                    2,
                    "io_error",
                    &format!("cannot write {}: {e}", path.display()),
                    None,
                    None,
                );
            }
            false
        }
        Err(e) => exit_error(
            json,
            2,
            "io_error",
            &format!("cannot write {}: {e}", path.display()),
            None,
            None,
        ),
    };
    // Prefer the canonical absolute path (the file now exists, so this resolves),
    // falling back to the join if canonicalization fails.
    let resolved = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    (resolved, created)
}

pub fn skill(install_claude: bool, user: bool, agent: SkillAgent, json: bool) {
    let skill_content = include_str!("skill.md");

    // `--user` requires `--install-claude` (clap-enforced), so `install_claude`
    // alone gates the install branch. Without it we just print the skill.
    if !install_claude {
        print!("{skill_content}");
        return;
    }

    let scope = if user { "user" } else { "project" };

    // Resolve HOME once — both agent targets share it under `--user`. A missing
    // home dir is a system-level failure the caller cannot fix by correcting
    // input → structured error, exit 2 (never panic, which would bypass the
    // --json contract with a raw backtrace).
    let home = if user {
        match dirs::home_dir() {
            Some(h) => Some(h),
            None => exit_error(
                json,
                2,
                "home_dir_not_found",
                "cannot determine home directory for a --user install ($HOME unset)",
                None,
                None,
            ),
        }
    } else {
        None
    };

    let want_claude = matches!(agent, SkillAgent::Claude | SkillAgent::All);
    let want_pi = matches!(agent, SkillAgent::Pi | SkillAgent::All);

    // Claude is written first so its top-level envelope fields (path/scope/created)
    // and the human "Installed skill to …" line stay backward-compatible: an
    // existing `skill --install-claude [--user]` invocation writes the same Claude
    // path with the same reported shape; the pi target is added alongside it.
    let mut targets: Vec<(&str, PathBuf, bool)> = Vec::new();

    if want_claude {
        let base = match &home {
            Some(h) => h.join(".claude"),
            None => {
                // Project scope keeps the "are you in a project root?" guard: a
                // missing `.claude/` is a user error (exit 1), not a system fault.
                let claude_dir = PathBuf::from(".claude");
                if !claude_dir.exists() {
                    exit_error(
                        json,
                        1,
                        "claude_dir_not_found",
                        ".claude/ directory not found in current directory. \
                         Are you in a project root? Use --install-claude --user for a user-level install.",
                        None,
                        None,
                    );
                }
                claude_dir
            }
        };
        let (path, created) = write_skill_file(&base.join("skills/glasspad"), skill_content, json);
        targets.push(("claude", path, created));
    }

    if want_pi {
        // pi.dev: `~/.pi/agent/skills/` (user) or `./.pi/skills/` (project). Unlike
        // Claude's project guard, the pi project dir is created on demand — pi has
        // no established project-root marker to key off, and creating `.pi/skills/`
        // is the least-surprise behavior for a pi-only project install.
        let base = match &home {
            Some(h) => h.join(".pi/agent"),
            None => PathBuf::from(".pi"),
        };
        let (path, created) = write_skill_file(&base.join("skills/glasspad"), skill_content, json);
        targets.push(("pi", path, created));
    }

    if json {
        // Per AI-first §10: report every path written — never silently write a
        // second target the envelope omits. The top-level path/created describe the
        // first target (Claude when present) for backward compatibility; `targets`
        // enumerates all of them.
        let targets_json: Vec<_> = targets
            .iter()
            .map(|(a, p, c)| {
                json!({
                    "agent": a,
                    "scope": scope,
                    "path": p.display().to_string(),
                    "created": c,
                })
            })
            .collect();
        let (_, first_path, first_created) = &targets[0];
        let payload = json!({
            "schema_version": SCHEMA_VERSION,
            "installed": true,
            "scope": scope,
            "path": first_path.display().to_string(),
            "created": first_created,
            "targets": targets_json,
            "cli_version": env!("CARGO_PKG_VERSION"),
            // Present (empty) for cross-command uniformity: callers read
            // `warnings` unconditionally across every envelope (see `data`).
            "warnings": [],
        });
        emit_json_line(&payload);
    } else {
        for (_, path, _) in &targets {
            println!("Installed skill to {}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed `chunks` to a fresh decoder and collect the ids it surfaces.
    fn decode_ids(chunks: &[&[u8]]) -> Result<Vec<u64>, ()> {
        let mut dec = SseDecoder::default();
        let mut out = Vec::new();
        for c in chunks {
            dec.feed(c, &mut out).map_err(|_| ())?;
        }
        Ok(out
            .into_iter()
            .map(|SseItem::Submission { id, .. }| id)
            .collect())
    }

    #[test]
    fn sse_decoder_reassembles_a_utf8_char_split_across_chunks() {
        // A submission whose data contains a multi-byte char (€ = 3 bytes) split at an
        // arbitrary byte boundary must decode intact — the old per-chunk lossy decode
        // corrupted it. Frame: `event: submission\ndata: {"id":7,"v":"€"}\n\n`.
        let frame = b"event: submission\ndata: {\"id\":7,\"v\":\"\xe2\x82\xac\"}\n\n";
        // Split mid-way through the € bytes.
        let cut = frame.iter().position(|&b| b == 0xe2).unwrap() + 1;
        let mut dec = SseDecoder::default();
        let mut out = Vec::new();
        dec.feed(&frame[..cut], &mut out).unwrap();
        dec.feed(&frame[cut..], &mut out).unwrap();
        assert_eq!(out.len(), 1);
        let SseItem::Submission { id, value } = &out[0];
        assert_eq!(*id, 7);
        assert_eq!(value["v"], "€", "the split multi-byte char is intact");
    }

    #[test]
    fn sse_decoder_handles_crlf_comments_and_ignores_non_submission() {
        // CRLF endings, keep-alive comment lines, and a non-`submission` event are all
        // tolerated; only the numeric-id submission event is surfaced.
        let ids = decode_ids(&[
            b": keep-alive\r\n",
            b"event: reload\r\ndata: 1\r\n\r\n",
            b"event: submission\r\nid: 4\r\ndata: {\"id\":4}\r\n\r\n",
        ])
        .unwrap();
        assert_eq!(ids, vec![4]);
    }

    #[test]
    fn sse_decoder_skips_a_submission_without_a_numeric_id() {
        // A degraded/id-less `submission` event is NOT surfaced (the client must never
        // count it as a real submission or fail to advance its cursor).
        let ids = decode_ids(&[b"event: submission\ndata: {\"no\":\"id\"}\n\n"]).unwrap();
        assert!(ids.is_empty());
    }

    #[test]
    fn sse_decoder_bounds_an_oversize_line() {
        // A hostile/broken peer streaming a line with no newline past the cap fails
        // closed (Err), never growing memory without bound.
        let big = vec![b'a'; MAX_SSE_LINE_BYTES + 1];
        let mut dec = SseDecoder::default();
        let mut out = Vec::new();
        assert!(dec.feed(&big, &mut out).is_err());
    }

    #[test]
    fn sse_decoder_streams_multiple_events_from_one_chunk() {
        let ids = decode_ids(&[
            b"event: submission\ndata: {\"id\":1}\n\nevent: submission\ndata: {\"id\":2}\n\n",
        ])
        .unwrap();
        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn detected_kind_matches_wrap_classifier() {
        // `create` reports the same authoring level the content route acts on.
        assert!(wrap::is_fragment("<h1>hi</h1>"));
        assert!(!wrap::is_fragment("<!doctype html><html></html>"));
        // BOM + whitespace + leading comment before a real doctype → full document.
        assert!(!wrap::is_fragment(
            "\u{feff}  <!-- x -->\n<!DOCTYPE HTML><html>…"
        ));
    }

    #[test]
    fn one_artifact_snapshot_home_and_title() {
        let snap = server::one_artifact_snapshot("report", "<title>Q3</title><h1>x</h1>".into());
        let sp = snap.space("report").unwrap();
        assert_eq!(sp.home.as_deref(), Some(server::SINGLE_SLUG));
        assert_eq!(sp.nav, vec![server::SINGLE_SLUG.to_string()]);
        assert_eq!(sp.artifact(server::SINGLE_SLUG).unwrap().title, "Q3");
    }

    #[test]
    fn one_artifact_title_falls_back_to_space_name() {
        let snap = server::one_artifact_snapshot("myspace", "<p>no title here</p>".into());
        assert_eq!(
            snap.space("myspace")
                .unwrap()
                .artifact(server::SINGLE_SLUG)
                .unwrap()
                .title,
            "myspace"
        );
    }

    #[test]
    fn publish_config_prefers_xdg_then_falls_back_to_platform_dir() {
        let cfg = |p: &str| PathBuf::from(p).join("glasspad").join("config.yaml");

        // $XDG_CONFIG_HOME (absolute) wins; platform dir follows as fallback.
        assert_eq!(
            publish_config_candidates_from(
                Some(PathBuf::from("/xdg")),
                Some(PathBuf::from("/home/u")),
                Some(PathBuf::from("/home/u/Library/Application Support")),
            ),
            vec![cfg("/xdg"), cfg("/home/u/Library/Application Support")],
        );

        // No XDG → ~/.config first, then the platform dir as backward-compat fallback.
        assert_eq!(
            publish_config_candidates_from(
                None,
                Some(PathBuf::from("/home/u")),
                Some(PathBuf::from("/home/u/Library/Application Support")),
            ),
            vec![
                cfg("/home/u/.config"),
                cfg("/home/u/Library/Application Support")
            ],
        );

        // A relative XDG value is ignored (falls through to ~/.config). On Unix
        // `dirs::config_dir()` echoes the same relative value; it must NOT become a
        // CWD-relative candidate — only the absolute ~/.config path survives.
        assert_eq!(
            publish_config_candidates_from(
                Some(PathBuf::from("relative/dir")),
                Some(PathBuf::from("/home/u")),
                Some(PathBuf::from("relative/dir")),
            ),
            vec![cfg("/home/u/.config")],
        );

        // On Linux the XDG path and platform dir coincide → no duplicate candidate.
        assert_eq!(
            publish_config_candidates_from(
                None,
                Some(PathBuf::from("/home/u")),
                Some(PathBuf::from("/home/u/.config")),
            ),
            vec![cfg("/home/u/.config")],
        );
    }

    #[test]
    fn data_format_inferred_from_extension() {
        assert_eq!(detect_data_format(Path::new("x.csv")), Some("csv"));
        assert_eq!(detect_data_format(Path::new("x.JSON")), Some("json")); // case-insensitive
        assert_eq!(detect_data_format(Path::new("mail.mbox")), Some("mbox"));
        assert_eq!(detect_data_format(Path::new("one.eml")), Some("mbox"));
        assert_eq!(detect_data_format(Path::new("notes.txt")), None);
        assert_eq!(detect_data_format(Path::new("noext")), None);
    }

    #[test]
    fn env_port_parsing_is_strict() {
        // Valid values pass through.
        assert_eq!(parse_env_port("8080"), Ok(8080));
        assert_eq!(parse_env_port("1"), Ok(1));
        assert_eq!(parse_env_port("65535"), Ok(65535));
        assert_eq!(parse_env_port("  3000\n"), Ok(3000)); // surrounding whitespace trimmed
        // Invalid values are rejected with an informative, value-naming message —
        // never coerced or silently defaulted (AI-first §1).
        for bad in [
            "", "   ", "0", "65536", "99999", "-1", "80abc", "abc", "3.14",
        ] {
            let err = parse_env_port(bad).unwrap_err();
            assert!(
                err.contains(PORT_ENV),
                "error for {bad:?} should name the env var: {err}"
            );
        }
        // Out-of-range vs. malformed get distinct diagnostics (AI-first §4).
        assert!(
            parse_env_port("65536")
                .unwrap_err()
                .contains("out of range")
        );
        assert!(
            parse_env_port("99999")
                .unwrap_err()
                .contains("out of range")
        );
        assert!(parse_env_port("0").unwrap_err().contains("out of range"));
        assert!(
            parse_env_port("abc")
                .unwrap_err()
                .contains("not a valid port")
        );
    }

    #[test]
    fn resolve_port_flag_wins_env_independent() {
        // An explicit flag is returned verbatim without consulting the environment,
        // so `--port` always beats `$GLASSPAD_PORT` (AI-first §8). The env→default
        // fallback is covered by `env_port_parsing_is_strict` (the pure parser); we
        // deliberately do not mutate the process environment in tests (unsafe +
        // racy under the parallel test harness).
        assert_eq!(resolve_port(Some(4100), false), 4100);
        assert_eq!(resolve_port(Some(1), true), 1);
    }

    #[test]
    fn cli_and_router_share_one_space_grammar() {
        // The names `open`/`create` accept are exactly what the router serves.
        assert!(artifact_host::valid_space("sales-q3"));
        assert!(!artifact_host::valid_space("api")); // reserved
        assert!(!artifact_host::valid_space("Bad_Name")); // grammar
        assert!(!artifact_host::valid_space("")); // empty
    }
}
