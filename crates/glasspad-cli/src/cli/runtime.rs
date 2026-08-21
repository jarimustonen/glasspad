use super::*;

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
pub(super) fn parse_env_port(raw: &str) -> Result<u16, String> {
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
pub(super) async fn acquire_pidfile(json: bool) -> Vec<String> {
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
pub(super) fn install_signal_cleanup(me: u32) {
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
pub(super) fn install_signal_cleanup(_me: u32) {}

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
pub(super) fn no_running_server_stale(json: bool, pid: u32, why: &str) -> ! {
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
pub(super) fn pid_path_display() -> String {
    pidfile::path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "~/.glasspad/server.pid".to_string())
}

/// Print the `stop` result envelope. Success is a terminal result (not long-running),
/// so `--json` emits a one-line result object; text prints a concise confirmation.
/// `stopped: true` means SIGTERM was delivered to the server (the `signal` field
/// names it); the server then shuts down and removes its own pid file.
#[cfg(unix)]
pub(super) fn emit_stopped(json: bool, pid: u32) {
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
pub(super) fn exit_scan_error(e: &ScanError, json: bool) -> ! {
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
        ScanError::UnknownTemplate(_) => ("unknown_template", 1),
        ScanError::TemplateNotFound(_) => ("template_not_found", 1),
        ScanError::TemplatePath(_) => ("invalid_template_path", 1),
        ScanError::TemplateFullDocument(_) => ("invalid_template", 1),
        ScanError::RenderTooLarge(_, _) => ("render_too_large", 1),
        ScanError::TemplateRender(_, _) => ("invalid_template", 1),
    };
    exit_error(json, exit, code, &e.to_string(), None, None);
}

/// Print a JSON envelope line to stdout and flush it. The flush matters for the
/// long-running `serve`/`create`: their startup envelope must reach a piped
/// consumer *before* the process blocks serving, not sit in a block buffer.
pub(super) fn emit_json_line(payload: &serde_json::Value) {
    use std::io::Write;
    let s = serde_json::to_string(payload).unwrap_or_default();
    println!("{s}");
    let _ = std::io::stdout().flush();
}

/// Emit the structured-help document through the same success envelope as the
/// other read-only CLI surfaces.
pub fn help_json(data: serde_json::Value) {
    emit_json_line(&json!({
        "schema_version": SCHEMA_VERSION,
        "data": data,
        "warnings": [],
    }));
}
