//! End-to-end contract tests for the loopback server's process-management
//! surface: the pid file, `glasspad stop`, and `$GLASSPAD_PORT`.
//!
//! Drives the built binary (`CARGO_BIN_EXE_glasspad`) so these exercise the real
//! CLI. Every test isolates its pid file via `$GLASSPAD_PID_FILE` (a unique temp
//! path) so a run can never touch — or be perturbed by — the user's real
//! `~/.glasspad/server.pid`. A unique loopback port per test keeps concurrent
//! `cargo test` runs from colliding.
//!
//! Contract under test:
//! * `serve` writes its PID to the pid file on start and removes it on a clean
//!   SIGTERM shutdown (what `stop` sends).
//! * `stop` finds the running server, signals it (exit 143 = SIGTERM), and reports
//!   `{stopped, pid, signal}`.
//! * `stop` with no server — or a *stale* pid file (recorded process dead) — is an
//!   informative `no_running_server` error (exit 1), and a stale file is cleaned.
//! * `$GLASSPAD_PORT` sets the port; an explicit `--port` beats it; an invalid
//!   value is a hard `invalid_port` error (never a silent default).

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_glasspad"))
}

/// A unique temp pid-file path (never created — just a name the server owns).
fn temp_pid_path(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("gp-it-pid-{tag}-{}-{nanos}", std::process::id()))
}

/// Spawn `glasspad serve --json --port <port>` with an isolated pid file and wait
/// until it answers (or panic after a timeout). Returns the child handle.
fn spawn_serve(port: u16, pid_file: &Path) -> Child {
    let child = bin()
        .args(["serve", "--json", "--port", &port.to_string()])
        .env("GLASSPAD_PID_FILE", pid_file)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn serve");
    wait_until(Duration::from_secs(10), || {
        std::net::TcpStream::connect(("127.0.0.1", port)).is_ok()
    });
    child
}

/// Poll `cond` until true or the deadline elapses (panics on timeout).
fn wait_until(max: Duration, mut cond: impl FnMut() -> bool) {
    let start = Instant::now();
    while start.elapsed() < max {
        if cond() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("condition not met within {max:?}");
}

/// Run `glasspad stop --json` against `pid_file` and return the (status, parsed
/// stdout-or-stderr JSON). `stop` prints the success envelope to stdout and the
/// error envelope to stderr, so we parse whichever is non-empty.
fn run_stop(pid_file: &Path) -> (std::process::ExitStatus, serde_json::Value) {
    let out = bin()
        .args(["stop", "--json"])
        .env("GLASSPAD_PID_FILE", pid_file)
        .output()
        .expect("run stop");
    let body = if out.stdout.iter().any(|b| !b.is_ascii_whitespace()) {
        &out.stdout
    } else {
        &out.stderr
    };
    let v: serde_json::Value =
        serde_json::from_slice(body).unwrap_or_else(|e| panic!("stop json parse: {e}"));
    (out.status, v)
}

#[test]
fn serve_writes_pid_file_and_stop_removes_it() {
    let pid_file = temp_pid_path("lifecycle");
    let port = 39_411;
    let mut child = spawn_serve(port, &pid_file);

    // The pid file exists and records the server's real PID.
    assert!(pid_file.exists(), "serve must write the pid file");
    let recorded: u32 = std::fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .expect("pid file holds an integer");
    assert_eq!(recorded, child.id(), "pid file records the serve process");

    // Stop finds and signals it: success envelope + SIGTERM.
    let (status, v) = run_stop(&pid_file);
    assert!(status.success(), "stop should succeed: {status:?}");
    assert_eq!(v["stopped"], true);
    assert_eq!(v["pid"], recorded);
    assert_eq!(v["signal"], "SIGTERM");

    // The server exits and removes its own pid file on the clean SIGTERM shutdown.
    let exit = child.wait().expect("wait for serve to exit");
    assert_eq!(exit.code(), Some(143), "SIGTERM shutdown exits 143");
    wait_until(Duration::from_secs(5), || !pid_file.exists());
    assert!(
        !pid_file.exists(),
        "clean shutdown must remove the pid file"
    );
}

#[test]
fn stop_with_no_server_is_informative_error() {
    let pid_file = temp_pid_path("no-server");
    let _ = std::fs::remove_file(&pid_file); // ensure absent
    let (status, v) = run_stop(&pid_file);
    assert_eq!(status.code(), Some(1), "no server → exit 1");
    assert_eq!(v["error"]["code"], "no_running_server");
}

#[test]
fn stop_detects_and_cleans_a_stale_pid_file() {
    let pid_file = temp_pid_path("stale");
    // A PID that is (almost certainly) not a live process → stale, not "running".
    std::fs::write(&pid_file, "999999\n").unwrap();
    let (status, v) = run_stop(&pid_file);
    assert_eq!(
        status.code(),
        Some(1),
        "stale file → no running server (exit 1)"
    );
    assert_eq!(v["error"]["code"], "no_running_server");
    assert!(
        v["error"]["message"].as_str().unwrap().contains("stale"),
        "message should name the stale cleanup: {v}"
    );
    assert!(!pid_file.exists(), "the stale pid file must be removed");
}

#[test]
fn glasspad_port_sets_port_and_flag_wins() {
    // `open` resolves the same port precedence as `serve`, without binding — a fast,
    // side-effect-free way to assert the resolution.
    let env_only = bin()
        .args(["open", "demo", "--no-browser", "--json"])
        .env("GLASSPAD_PORT", "4242")
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&env_only.stdout).unwrap();
    assert_eq!(v["port"], 4242, "$GLASSPAD_PORT sets the port");

    let flag_wins = bin()
        .args(["open", "demo", "--no-browser", "--json", "--port", "5555"])
        .env("GLASSPAD_PORT", "4242")
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&flag_wins.stdout).unwrap();
    assert_eq!(v["port"], 5555, "explicit --port beats $GLASSPAD_PORT");
}

#[test]
fn invalid_glasspad_port_is_a_hard_error() {
    let out = bin()
        .args(["open", "demo", "--no-browser", "--json"])
        .env("GLASSPAD_PORT", "notaport")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "invalid env port → exit 1");
    let v: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(v["error"]["code"], "invalid_port");
    assert_eq!(v["error"]["invalid_value"], "notaport");
}
