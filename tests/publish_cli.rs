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

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_glasspad"))
}

struct KillOnDrop(Child);

impl std::ops::Deref for KillOnDrop {
    type Target = Child;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for KillOnDrop {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.kill();
        let _ = self.wait();
    }
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
        .env_remove("GLASSPAD_TARGET")
        .env_remove("GLASSPAD_SPACE_KEY")
        .env_remove("GLASSPAD_TEMPLATE")
        .env_remove("GLASSPAD_PORT");
    c
}

fn parse(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes)
        .unwrap_or_else(|e| panic!("json parse: {e}\n{}", String::from_utf8_lossy(bytes)))
}

const HOST_KEY: &str = "publish-test-key-0123456789abcdefghijkl";

fn spawn_host(root: &Path) -> (KillOnDrop, String) {
    let key_file = write(root, "keys.txt", &format!("tester:{HOST_KEY}\n"));
    let store = root.join("store");
    std::fs::create_dir_all(&store).unwrap();
    let mut child = KillOnDrop(
        bin()
            .args(["--json", "host-serve", "--bind", "127.0.0.1:0"])
            .arg("--api-key-file")
            .arg(key_file)
            .arg("--store")
            .arg(store)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn host-serve"),
    );

    let logs = Arc::new(Mutex::new(String::new()));
    let stderr = child.stderr.take().expect("host stderr");
    let stderr_logs = logs.clone();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = BufReader::new(stderr).read_to_string(&mut text);
        *stderr_logs.lock().expect("stderr lock") = text;
    });
    let stdout = child.stdout.take().expect("host stdout");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let result = reader.read_line(&mut line).map(|_| line);
        let _ = tx.send(result);
        let _ = std::io::copy(&mut reader, &mut std::io::sink());
    });
    let line = match rx.recv_timeout(Duration::from_secs(15)) {
        Ok(Ok(line)) if !line.is_empty() => line,
        result => {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "host-serve did not report startup ({result:?}); logs: {:?}",
                logs.lock().unwrap()
            );
        }
    };
    let startup: serde_json::Value = serde_json::from_str(&line).expect("host startup JSON");
    let bind: SocketAddr = startup["bind"].as_str().unwrap().parse().unwrap();
    (child, format!("http://{bind}"))
}

fn hosted_publish(
    cwd: &Path,
    home: &Path,
    server: &str,
    path: &Path,
    extra: &[&str],
) -> serde_json::Value {
    let mut cmd = hermetic(cwd, home);
    cmd.args([
        "--json",
        "publish",
        "--no-open",
        "--target",
        "hosted",
        "--server",
        server,
        "--api-key",
        HOST_KEY,
    ])
    .arg(path)
    .args(extra);
    let out = cmd.output().expect("run hosted publish");
    assert!(
        out.status.success(),
        "publish failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    parse(&out.stdout)
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
fn repeated_hosted_publish_uses_source_identity_and_new_is_explicit() {
    let dir = tmp_dir("stable-source");
    let home = tmp_dir("stable-source-home");
    let host_root = tmp_dir("stable-source-host");
    let (_host, server) = spawn_host(&host_root);
    let page = write(&dir, "page.md", "# First\n");

    // Relative and absolute spellings canonicalize to one source identity.
    let first = hosted_publish(&dir, &home, &server, Path::new("page.md"), &[]);
    write(&dir, "page.md", "# Second\n");
    let repeated = hosted_publish(&dir, &home, &server, &page, &[]);
    assert_eq!(first["slug"], repeated["slug"]);
    assert_eq!(first["created"], true);
    assert_eq!(repeated["created"], false);

    // Moving a source intentionally gives it a new path identity.
    let moved = dir.join("moved.md");
    std::fs::rename(&page, &moved).unwrap();
    let moved_publish = hosted_publish(&dir, &home, &server, &moved, &[]);
    assert_ne!(moved_publish["slug"], first["slug"]);
    assert_eq!(moved_publish["created"], true);

    // Creating another space at the same path requires the plainly named explicit path.
    let fresh = hosted_publish(&dir, &home, &server, &moved, &["--new"]);
    assert_ne!(fresh["slug"], moved_publish["slug"]);
    assert_eq!(fresh["created"], true);

    // Explicit update still targets exactly the requested pre-existing URL, even
    // after the source moved.
    let slug = first["slug"].as_str().unwrap();
    let updated = hosted_publish(&dir, &home, &server, &moved, &["--update", slug]);
    assert_eq!(updated["slug"], first["slug"]);
    assert_eq!(updated["created"], false);
    let after_adoption = hosted_publish(&dir, &home, &server, &moved, &[]);
    assert_eq!(after_adoption["slug"], first["slug"]);
    assert_eq!(after_adoption["created"], false);
}

#[test]
fn new_and_space_key_together_are_rejected_by_clap() {
    let dir = tmp_dir("new-conflict");
    let home = tmp_dir("new-conflict-home");
    let md = write(&dir, "page.md", "# Hi\n");
    let out = hermetic(&dir, &home)
        .args(["--json", "publish"])
        .arg(&md)
        .args(["--target", "hosted", "--new", "--space-key", "k"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "clap arg conflict → exit 2");
}

#[test]
fn new_flag_on_loopback_target_is_rejected() {
    let dir = tmp_dir("new-loopback");
    let home = tmp_dir("new-loopback-home");
    let md = write(&dir, "page.md", "# Hi\n");
    let out = hermetic(&dir, &home)
        .args(["--json", "publish"])
        .arg(&md)
        .arg("--new")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(parse(&out.stderr)["error"]["code"], "option_not_applicable");
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
fn update_invalid_grammar_slug_is_rejected_locally_before_network() {
    // `--update` is validated against the capability-slug grammar CLIENT-SIDE, and
    // BEFORE server/key resolution — an uppercase/reserved/traversal value is a
    // deterministic local `invalid_update_slug`, never a `missing_server` or a network
    // round-trip. No server is configured, so reaching `invalid_update_slug` proves the
    // grammar gate runs first.
    let dir = tmp_dir("update-badgrammar");
    let home = tmp_dir("update-badgrammar-home");
    let md = write(&dir, "page.md", "# Hi\n");
    for bad in ["Upperslug", "../pages", "has/slash", "api"] {
        let out = hermetic(&dir, &home)
            .args(["--json", "publish"])
            .arg(&md)
            .args(["--target", "hosted", "--update", bad])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1), "slug {bad:?} must be rejected");
        let v = parse(&out.stderr);
        assert_eq!(
            v["error"]["code"], "invalid_update_slug",
            "slug {bad:?} must fail local grammar validation, not reach the network"
        );
    }
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
