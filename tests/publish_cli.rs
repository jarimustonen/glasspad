//! End-to-end contract tests for the publish-first CLI surface.
//!
//! Drives the built binary (`CARGO_BIN_EXE_glasspad`). Every test is hermetic
//! about config: it points `$HOME` + `$XDG_CONFIG_HOME` at an *empty* temp dir so
//! no real `~/.config/glasspad/config.yaml` leaks in, and runs the child in a temp
//! CWD with no `.glasspad.yaml` above it — so config resolution sees the built-in
//! default (`target: loopback`) unless a test writes a config itself.
//!
//! Contract under test:
//! * `publish` classifies `<path>` (missing / unsupported / file / dir) with
//!   informative, stable error codes.
//! * With no config, the target defaults to loopback (zero-config local serve).
//! * `.glasspad.yaml` / `$GLASSPAD_TARGET` select the hosted target (asserted via
//!   the missing-server error, so the tests make no network call).

use std::io::Write;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_glasspad"))
}

/// A unique temp directory for one test (created).
fn tmp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let d = std::env::temp_dir().join(format!("gp-pub-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
    let p = dir.join(name);
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(body.as_bytes()).unwrap();
    p
}

/// A `glasspad` command with a hermetic, empty config environment and a CWD with no
/// `.glasspad.yaml` above it (unless the caller writes one into `cwd`).
fn hermetic(cwd: &Path, empty_home: &Path) -> Command {
    let mut c = bin();
    c.current_dir(cwd)
        .env("HOME", empty_home)
        .env("XDG_CONFIG_HOME", empty_home)
        .env_remove("GLASSPAD_SERVER")
        .env_remove("GLASSPAD_API_KEY")
        .env_remove("GLASSPAD_TARGET");
    c
}

fn parse(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes)
        .unwrap_or_else(|e| panic!("json parse: {e}\n{}", String::from_utf8_lossy(bytes)))
}

fn wait_until(max: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < max {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn publish_missing_path_is_no_such_path() {
    let dir = tmp_dir("missing");
    let home = tmp_dir("missing-home");
    let out = hermetic(&dir, &home)
        .args(["--json", "publish", "does-not-exist.md"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "missing path → exit 1");
    let v = parse(&out.stderr);
    assert_eq!(v["error"]["code"], "no_such_path");
}

#[test]
fn publish_unsupported_extension_is_rejected() {
    let dir = tmp_dir("unsupported");
    let home = tmp_dir("unsupported-home");
    let f = write(&dir, "notes.txt", "hello");
    let out = hermetic(&dir, &home)
        .args(["--json", "publish"])
        .arg(&f)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "unsupported ext → exit 1");
    let v = parse(&out.stderr);
    assert_eq!(v["error"]["code"], "unsupported_input");
}

#[test]
fn publish_hosted_without_server_is_missing_server() {
    // `.glasspad.yaml target: hosted` selects the hosted target; with no server
    // configured anywhere, publish fails fast with `missing_server` (no network).
    let dir = tmp_dir("hosted");
    let home = tmp_dir("hosted-home");
    write(&dir, ".glasspad.yaml", "target: hosted\n");
    let md = write(&dir, "page.md", "# Title\n\nBody.\n");
    let out = hermetic(&dir, &home)
        .args(["--json", "publish"])
        .arg(&md)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "hosted + no server → exit 1");
    let v = parse(&out.stderr);
    assert_eq!(v["error"]["code"], "missing_server");
}

#[test]
fn publish_target_flag_overrides_to_hosted() {
    // `--target hosted` overrides config (there is none); same missing-server result.
    let dir = tmp_dir("flag");
    let home = tmp_dir("flag-home");
    let md = write(&dir, "page.md", "# Hi\n");
    let out = hermetic(&dir, &home)
        .args(["--json", "publish"])
        .arg(&md)
        .args(["--target", "hosted"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let v = parse(&out.stderr);
    assert_eq!(v["error"]["code"], "missing_server");
}

#[test]
fn publish_invalid_target_is_rejected() {
    let dir = tmp_dir("badtarget");
    let home = tmp_dir("badtarget-home");
    let md = write(&dir, "page.md", "# Hi\n");
    let out = hermetic(&dir, &home)
        .args(["--json", "publish"])
        .arg(&md)
        .args(["--target", "nowhere"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let v = parse(&out.stderr);
    assert_eq!(v["error"]["code"], "invalid_target");
}

#[test]
fn explicit_template_on_html_is_rejected() {
    // `--template` is only valid for a single markdown file (not silently ignored).
    let dir = tmp_dir("tmpl-html");
    let home = tmp_dir("tmpl-html-home");
    let f = write(&dir, "page.html", "<h1>hi</h1>");
    let out = hermetic(&dir, &home)
        .args(["--json", "publish"])
        .arg(&f)
        .args(["--template", "prose"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let v = parse(&out.stderr);
    assert_eq!(v["error"]["code"], "template_not_applicable");
}

#[test]
fn config_default_template_does_not_break_html_publish() {
    // A config `template:` default must NOT make publishing raw .html fail: it is a
    // fallback for markdown only. Publish .html hosted → the run reaches server
    // resolution (missing_server), never a template error.
    let dir = tmp_dir("tmpl-cfg");
    let home = tmp_dir("tmpl-cfg-home");
    write(&dir, ".glasspad.yaml", "target: hosted\ntemplate: prose\n");
    let f = write(&dir, "page.html", "<h1>hi</h1>");
    let out = hermetic(&dir, &home)
        .args(["--json", "publish"])
        .arg(&f)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let v = parse(&out.stderr);
    assert_eq!(
        v["error"]["code"], "missing_server",
        "a config template default must not turn into a template error for .html"
    );
}

#[test]
fn update_and_space_key_together_are_rejected_by_clap() {
    // `--update` and `--space-key` are two ways to say "update in place"; clap's
    // `conflicts_with` rejects passing both (exit 2, argument error).
    let dir = tmp_dir("update-conflict");
    let home = tmp_dir("update-conflict-home");
    let md = write(&dir, "page.md", "# Hi\n");
    let out = hermetic(&dir, &home)
        .args(["--json", "publish"])
        .arg(&md)
        .args([
            "--target",
            "hosted",
            "--update",
            "abcdef",
            "--space-key",
            "k",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "clap arg conflict → exit 2");
}

#[test]
fn update_flag_on_loopback_target_is_rejected() {
    // `--update` is hosted-only; on a loopback-resolved publish it is a usage error.
    let dir = tmp_dir("update-loopback");
    let home = tmp_dir("update-loopback-home");
    let md = write(&dir, "page.md", "# Hi\n");
    let out = hermetic(&dir, &home)
        .args(["--json", "publish"])
        .arg(&md)
        .args(["--update", "abcdef"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let v = parse(&out.stderr);
    assert_eq!(v["error"]["code"], "option_not_applicable");
}

#[test]
fn update_empty_slug_is_rejected() {
    // A whitespace-only `--update` value is a caller bug, rejected strictly.
    let dir = tmp_dir("update-empty");
    let home = tmp_dir("update-empty-home");
    write(
        &dir,
        ".glasspad.yaml",
        "target: hosted\nserver: https://pad.example\n",
    );
    let md = write(&dir, "page.md", "# Hi\n");
    let out = hermetic(&dir, &home)
        .args(["--json", "publish"])
        .arg(&md)
        .args(["--api-key", "sk", "--update", "   "])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let v = parse(&out.stderr);
    assert_eq!(v["error"]["code"], "invalid_update_slug");
}

#[test]
fn hosted_only_flag_on_loopback_target_is_rejected() {
    // `--server` on a loopback-resolved publish is a usage error, not a silent no-op.
    let dir = tmp_dir("opt");
    let home = tmp_dir("opt-home");
    let md = write(&dir, "page.md", "# Hi\n");
    let out = hermetic(&dir, &home)
        .args(["--json", "publish"])
        .arg(&md)
        .args(["--server", "https://pad.example"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let v = parse(&out.stderr);
    assert_eq!(v["error"]["code"], "option_not_applicable");
}

#[test]
fn publish_zero_config_defaults_to_loopback_serve() {
    // Done-criteria #1: with NO config at all, `publish <dir>` serves loopback.
    // Spawn it (blocking, live-reload) on an isolated pid file + port, confirm it
    // binds, then stop it via `loopback stop`.
    let dir = tmp_dir("loopback");
    let home = tmp_dir("loopback-home");
    let space = dir.join("myspace");
    std::fs::create_dir_all(&space).unwrap();
    write(&space, "index.html", "<h1>hello</h1>");
    let pid_file = dir.join("server.pid");
    let port: u16 = 39_531;

    let mut child: Child = hermetic(&dir, &home)
        .args(["--json", "publish"])
        .arg(&space)
        .args(["--no-open", "--port", &port.to_string()])
        .env("GLASSPAD_PID_FILE", &pid_file)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn publish (loopback)");

    let bound = wait_until(Duration::from_secs(10), || {
        TcpStream::connect(("127.0.0.1", port)).is_ok()
    });
    assert!(bound, "publish (loopback default) must bind and serve");

    // Stop it via the loopback management group.
    let stop = hermetic(&dir, &home)
        .args(["--json", "loopback", "stop"])
        .env("GLASSPAD_PID_FILE", &pid_file)
        .output()
        .unwrap();
    assert!(
        stop.status.success(),
        "loopback stop should succeed: {:?}",
        stop.status
    );
    let _ = child.wait();
}
