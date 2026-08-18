//! End-to-end contract tests for the return-channel *returning-agent* surface:
//! `glasspad submissions <slug>` (drain the durable backlog) and the
//! `await-submission` invocation that `publish` prints after a hosted publish.
//!
//! These drive the built binary (`CARGO_BIN_EXE_glasspad`) against a REAL hosted
//! server spawned as a subprocess (`host-serve`), so they exercise the whole path:
//! publish → a shell-style submit POST → drain over the API-key-authenticated,
//! per-tenant-scoped read route.
//!
//! Contract under test:
//! * `publish` (hosted) prints a copy-pasteable `await-submission` + `submissions`
//!   invocation carrying the *configured public host* and the assigned slug, plus
//!   the server's retention window.
//! * `submissions <slug>` drains everything already stored for a page in one
//!   non-blocking poll, exit 0 whether the backlog is empty or not.
//! * The drain is per-tenant scoped: a slug the key's tenant does not own is an
//!   opaque `no_such_page` (exit 1) — never a cross-tenant read.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_glasspad"))
}

/// A unique temp directory for one test (created).
fn tmp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let d = std::env::temp_dir().join(format!("gp-subs-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
    let p = dir.join(name);
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(body.as_bytes()).unwrap();
    p
}

/// A high-entropy (≥32-char) API key per tenant, as the key file requires.
const KEY_A: &str = "alice-key-0123456789abcdefghijklmnop";
const KEY_B: &str = "bob-key-0123456789abcdefghijklmnopqrst";

fn read_startup(child: &mut Child) -> serde_json::Value {
    let logs = Arc::new(Mutex::new(String::new()));
    let stderr = child.stderr.take().expect("host stderr");
    let stderr_logs = logs.clone();
    let stderr_thread = std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut text = String::new();
        let _ = reader.read_to_string(&mut text);
        *stderr_logs.lock().expect("stderr log lock") = text;
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
            let _ = stderr_thread.join();
            let stderr = logs.lock().expect("stderr log lock").clone();
            panic!("host-serve did not report its bind ({result:?}); stderr: {stderr}");
        }
    };
    match serde_json::from_str(&line) {
        Ok(value) => value,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("invalid host startup JSON ({e}): {line:?}");
        }
    }
}

/// Spawn `host-serve` on an OS-assigned loopback port, read its flushed startup
/// envelope, and return the child plus the origin derived from the reported bind.
fn spawn_host(tag: &str, retention_days: i64) -> (Child, String, PathBuf) {
    let root = tmp_dir(tag);
    let key_file = write(&root, "keys.txt", &format!("alice:{KEY_A}\nbob:{KEY_B}\n"));
    let store = root.join("store");
    std::fs::create_dir_all(&store).unwrap();
    let mut child = bin()
        .args(["--json", "host-serve", "--bind", "127.0.0.1:0"])
        .arg("--api-key-file")
        .arg(&key_file)
        .arg("--store")
        .arg(&store)
        .args(["--retention-days", &retention_days.to_string()])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn host-serve");

    let startup = read_startup(&mut child);
    let bind: SocketAddr = startup["bind"]
        .as_str()
        .expect("startup bind")
        .parse()
        .expect("startup bind address");
    assert_eq!(bind.ip().to_string(), "127.0.0.1");
    assert_ne!(
        bind.port(),
        0,
        "reported bind must contain the assigned port"
    );
    let server = format!("http://{bind}");
    assert_eq!(startup["public_host"].as_str(), Some(server.as_str()));

    (child, server, root)
}

/// A hermetic `glasspad` command: no ambient config/env can leak the server/key in.
fn hermetic(cwd: &Path) -> Command {
    let home = tmp_dir("home");
    let mut c = bin();
    c.current_dir(cwd)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &home)
        .env_remove("GLASSPAD_SERVER")
        .env_remove("GLASSPAD_API_KEY")
        .env_remove("GLASSPAD_TARGET");
    c
}

fn parse(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes)
        .unwrap_or_else(|e| panic!("json parse: {e}\n{}", String::from_utf8_lossy(bytes)))
}

/// Publish a one-page space to `server` as tenant A and return the parsed JSON.
fn publish(server: &str, page_dir: &Path) -> serde_json::Value {
    let out = hermetic(page_dir)
        .args([
            "--json",
            "publish",
            "--no-open",
            "--target",
            "hosted",
            "--server",
            server,
            "--api-key",
            KEY_A,
        ])
        .arg(page_dir.join("index.html"))
        .output()
        .expect("run publish");
    assert!(
        out.status.success(),
        "publish must succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    parse(&out.stdout)
}

/// POST a shell-style submission to a page (same-origin, no API key — the CSRF gate
/// only requires a matching `Origin`). Returns the HTTP status code.
fn submit(server: &str, slug: &str, data: serde_json::Value) -> u16 {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        let resp = client
            .post(format!("{server}/api/v1/pages/{slug}/submit"))
            .header(reqwest::header::ORIGIN, server)
            .json(&serde_json::json!({ "data": data }))
            .send()
            .await
            .expect("submit request");
        resp.status().as_u16()
    })
}

/// Run `glasspad submissions <slug>` as `key`, returning (exit code, stdout JSON
/// lines parsed, parsed error-or-envelope from whichever stream is non-empty).
fn drain(server: &str, slug: &str, key: &str, extra: &[&str]) -> std::process::Output {
    let cwd = tmp_dir("cwd");
    hermetic(&cwd)
        .args([
            "--json",
            "submissions",
            slug,
            "--server",
            server,
            "--api-key",
            key,
        ])
        .args(extra)
        .output()
        .expect("run submissions")
}

#[test]
fn publish_prints_await_and_drain_invocations_with_reported_host() {
    let (mut child, server, _root) = spawn_host("publish-hint", 7);
    let page_dir = tmp_dir("page");
    write(&page_dir, "index.html", "<h1>form</h1>");

    let v = publish(&server, &page_dir);
    let slug = v["slug"].as_str().expect("slug").to_string();

    // The publish envelope carries a copy-pasteable await-submission + drain command
    // pinned to the reported public host (never a hardcoded one) and the slug.
    let await_cmd = v["await_submission"]
        .as_str()
        .expect("await_submission field");
    let drain_cmd = v["drain_submissions"]
        .as_str()
        .expect("drain_submissions field");
    assert!(
        await_cmd.contains("await-submission")
            && await_cmd.contains(&server)
            && await_cmd.contains(&slug),
        "await invocation must name the server + slug: {await_cmd}"
    );
    assert!(
        drain_cmd.contains("submissions")
            && drain_cmd.contains(&server)
            && drain_cmd.contains(&slug),
        "drain invocation must name the server + slug: {drain_cmd}"
    );
    // The API key must NEVER be printed into the invocation.
    assert!(
        !await_cmd.contains(KEY_A) && !drain_cmd.contains(KEY_A),
        "the API key must never appear in a printed invocation"
    );
    // The retention window is surfaced (the configured 7 days).
    assert_eq!(v["retention_days"].as_i64(), Some(7));
    assert!(
        v["submissions_note"]
            .as_str()
            .unwrap_or_default()
            .contains("7 day"),
        "retention note must state the configured window: {v}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn submissions_drains_backlog_and_enforces_tenant_isolation() {
    let (mut child, server, _root) = spawn_host("drain", 90);
    let page_dir = tmp_dir("page");
    write(&page_dir, "index.html", "<h1>form</h1>");

    let v = publish(&server, &page_dir);
    let slug = v["slug"].as_str().expect("slug").to_string();

    // Empty backlog first: a drain before any submission is exit 0 with no rows.
    let out = drain(&server, &slug, KEY_A, &[]);
    assert_eq!(out.status.code(), Some(0), "empty drain is exit 0");
    let env = parse(&out.stdout);
    assert_eq!(env["submissions"].as_array().map(|a| a.len()), Some(0));

    // Two visitors submit while no agent is listening — they land in the store.
    assert_eq!(
        submit(&server, &slug, serde_json::json!({ "answer": "yes" })),
        201
    );
    assert_eq!(
        submit(&server, &slug, serde_json::json!({ "answer": "no" })),
        201
    );

    // The returning owner drains the whole backlog in one call.
    let out = drain(&server, &slug, KEY_A, &[]);
    assert_eq!(out.status.code(), Some(0));
    let env = parse(&out.stdout);
    let subs = env["submissions"].as_array().expect("submissions array");
    assert_eq!(subs.len(), 2, "both stored submissions are drained");
    assert_eq!(subs[0]["data"]["answer"], "yes");
    assert_eq!(subs[1]["data"]["answer"], "no");
    let cursor = env["cursor"].as_u64().expect("cursor");

    // `--since <cursor>` returns only newer rows (none) — still exit 0.
    let out = drain(&server, &slug, KEY_A, &["--since", &cursor.to_string()]);
    assert_eq!(out.status.code(), Some(0));
    let env = parse(&out.stdout);
    assert_eq!(env["submissions"].as_array().map(|a| a.len()), Some(0));

    // Per-tenant isolation: tenant B (bob) draining alice's slug gets an OPAQUE
    // not-found — never a cross-tenant read.
    let out = drain(&server, &slug, KEY_B, &[]);
    assert_eq!(out.status.code(), Some(1), "cross-tenant drain → exit 1");
    let err = parse(&out.stderr);
    assert_eq!(err["error"]["code"], "no_such_page");

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn explicit_public_host_stays_authoritative_with_ephemeral_bind() {
    let root = tmp_dir("explicit-origin");
    let key_file = write(&root, "keys.txt", &format!("alice:{KEY_A}\n"));
    let mut child = bin()
        .args(["--json", "host-serve", "--bind", "127.0.0.1:0"])
        .args(["--public-host", "https://glasspad.example.com"])
        .arg("--api-key-file")
        .arg(&key_file)
        .arg("--store")
        .arg(root.join("store"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn host-serve");
    let startup = read_startup(&mut child);
    let bind: SocketAddr = startup["bind"]
        .as_str()
        .expect("startup bind")
        .parse()
        .expect("startup bind address");
    assert_ne!(bind.port(), 0);
    assert_eq!(
        startup["public_host"].as_str(),
        Some("https://glasspad.example.com")
    );
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn host_serve_refuses_wildcard_with_ephemeral_port() {
    let root = tmp_dir("wildcard");
    let key_file = write(&root, "keys.txt", &format!("alice:{KEY_A}\n"));
    for bind in ["0.0.0.0:0", "[::ffff:0.0.0.0]:0"] {
        let out = bin()
            .args(["--json", "host-serve", "--bind", bind])
            .arg("--public-host")
            .arg("https://glasspad.example.com")
            .arg("--api-key-file")
            .arg(&key_file)
            .arg("--store")
            .arg(root.join("store"))
            .output()
            .expect("run host-serve with wildcard bind");
        assert_eq!(out.status.code(), Some(1), "bind {bind}");
        let err = parse(&out.stderr);
        assert_eq!(err["error"]["code"], "invalid_bind", "bind {bind}");
        assert!(
            err["error"]["message"]
                .as_str()
                .unwrap_or_default()
                .contains("wildcard"),
            "wildcard refusal must be informative: {err}"
        );
    }
}

#[test]
fn submissions_without_server_is_missing_server() {
    // Hosted-only: with no --server / $GLASSPAD_SERVER / config, the drain fails fast
    // with `missing_server` (there is no loopback backlog to drain), no network.
    let cwd = tmp_dir("cwd");
    let out = hermetic(&cwd)
        .args(["--json", "submissions", "somepage"])
        .output()
        .expect("run submissions");
    assert_eq!(out.status.code(), Some(1), "no server → exit 1");
    let err = parse(&out.stderr);
    assert_eq!(err["error"]["code"], "missing_server");
}

#[test]
fn submissions_invalid_slug_is_rejected_before_network() {
    // A malformed slug is a strict-validation error (exit 1) before any request.
    let cwd = tmp_dir("cwd");
    let out = hermetic(&cwd)
        .args([
            "--json",
            "submissions",
            "Bad Slug!",
            "--server",
            "https://pad.example.com",
            "--api-key",
            KEY_A,
        ])
        .output()
        .expect("run submissions");
    assert_eq!(out.status.code(), Some(1), "invalid slug → exit 1");
    let err = parse(&out.stderr);
    assert_eq!(err["error"]["code"], "invalid_slug");
}
