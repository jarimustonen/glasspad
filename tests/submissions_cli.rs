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

use std::io::Write;
use std::net::TcpListener;
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

/// Grab a currently-free loopback port by binding to :0 and immediately releasing
/// it. A short race window before `host-serve` rebinds is acceptable for tests.
fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
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

/// A high-entropy (≥32-char) API key per tenant, as the key file requires.
const KEY_A: &str = "alice-key-0123456789abcdefghijklmnop";
const KEY_B: &str = "bob-key-0123456789abcdefghijklmnopqrst";

/// Spawn `host-serve` on `port` with a two-tenant key file, wait until it binds, and
/// return the child + its public origin. The child is killed on drop by the caller.
fn spawn_host(tag: &str, port: u16, retention_days: i64) -> (Child, String, PathBuf) {
    let root = tmp_dir(tag);
    let key_file = write(&root, "keys.txt", &format!("alice:{KEY_A}\nbob:{KEY_B}\n"));
    let store = root.join("store");
    std::fs::create_dir_all(&store).unwrap();
    let public = format!("http://127.0.0.1:{port}");
    let child = bin()
        .args(["host-serve", "--bind"])
        .arg(format!("127.0.0.1:{port}"))
        .args(["--public-host", &public, "--api-key-file"])
        .arg(&key_file)
        .arg("--store")
        .arg(&store)
        .args(["--retention-days", &retention_days.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn host-serve");
    let bound = wait_until(Duration::from_secs(15), || {
        std::net::TcpStream::connect(("127.0.0.1", port)).is_ok()
    });
    assert!(bound, "host-serve must bind");
    (child, public, root)
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
fn publish_prints_await_and_drain_invocations_with_configured_host() {
    let port = free_port();
    let (mut child, server, _root) = spawn_host("publish-hint", port, 7);
    let page_dir = tmp_dir("page");
    write(&page_dir, "index.html", "<h1>form</h1>");

    let v = publish(&server, &page_dir);
    let slug = v["slug"].as_str().expect("slug").to_string();

    // The publish envelope carries a copy-pasteable await-submission + drain command
    // pinned to the CONFIGURED public host (never a hardcoded one) and the slug.
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
    let port = free_port();
    let (mut child, server, _root) = spawn_host("drain", port, 90);
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
