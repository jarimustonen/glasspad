//! End-to-end contract tests for `glasspad build <space> <out>`.
//!
//! Drives the built binary (`CARGO_BIN_EXE_glasspad`) so the tests exercise the
//! real CLI surface: the static render reuses the same security-checked scanner
//! `serve` uses (a reserved slug is refused before any output is written), wraps
//! each artifact through the same seam the content route uses, and — in the
//! default self-contained mode — bundles the base libs under `_gp/v1/` and
//! references them relatively so the output resolves offline.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_glasspad"))
}

/// A uniquely-named empty temp directory for one test (pid + tag, no randomness).
fn temp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("glasspad-build-test-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn write(dir: &Path, rel: &str, contents: &[u8]) {
    let p = dir.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, contents).unwrap();
}

/// Populate `sales` (the space name is the dir's final component) with a
/// fragment home, a full-document artifact, and an asset.
fn populate_space(space: &Path) {
    // A fragment (no <!doctype>): wrapped + bridged, base libs localized.
    write(space, "index.html", b"<h1>Home</h1><p>A fragment.</p>");
    // A full document: served verbatim (no injected bridge).
    write(
        space,
        "report.html",
        b"<!doctype html><html><head><title>Report</title></head>\
          <body><h1>Report</h1></body></html>",
    );
    write(space, "assets/data.json", b"{\"ok\":true}");
}

#[test]
fn self_contained_build_json_envelope_and_offline_libs() {
    let root = temp_dir("self-contained");
    let space = root.join("sales");
    std::fs::create_dir_all(&space).unwrap();
    populate_space(&space);
    let out = root.join("out");

    let res = bin()
        .arg("--json")
        .arg("build")
        .arg(&space)
        .arg(&out)
        .output()
        .unwrap();

    assert!(
        res.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&res.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&res.stdout).unwrap();
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["built"], true);
    assert_eq!(v["space"], "sales");
    assert_eq!(v["mode"], "self-contained");
    assert_eq!(v["base_libs_bundled"], true);
    assert_eq!(v["home"], "index");
    assert_eq!(v["index"], "index.html");
    assert_eq!(v["dry_run"], false);
    // A standing security/nav caveat is always present.
    let warnings = v["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("not sandboxed")
                || w.as_str().unwrap().contains("NOT sandboxed")),
        "warnings: {warnings:?}"
    );
    // Both artifacts are reported.
    let arts = v["artifacts"].as_array().unwrap();
    assert!(arts.iter().any(|s| s == "index"));
    assert!(arts.iter().any(|s| s == "report"));

    // The fragment home is wrapped, bridged, and references the base libs by a
    // RELATIVE path that actually exists on disk → resolves offline.
    let home = std::fs::read_to_string(out.join("index.html")).unwrap();
    assert!(home.starts_with("<!doctype html>"));
    assert!(home.contains("<h1>Home</h1>"));
    assert!(home.contains(r#"href="_gp/v1/base.css""#));
    assert!(home.contains(r#"src="_gp/v1/bridge.js""#));
    assert!(!home.contains(r#"href="/_gp/v1/base.css""#));
    assert!(out.join("_gp/v1/base.css").is_file());
    assert!(out.join("_gp/v1/bridge.js").is_file());
    // charts + vendored vega are bundled too (fully self-contained).
    assert!(out.join("_gp/v1/charts.js").is_file());
    assert!(out.join("_gp/v1/vega.min.js").is_file());

    // The full document is served verbatim (no injected bridge).
    let report = std::fs::read_to_string(out.join("report.html")).unwrap();
    assert!(report.contains("<h1>Report</h1>"));
    assert!(!report.contains("bridge.js"));

    // The asset is copied under its relative key.
    assert_eq!(
        std::fs::read(out.join("assets/data.json")).unwrap(),
        b"{\"ok\":true}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn shared_libs_build_keeps_absolute_refs_and_omits_libs() {
    let root = temp_dir("shared-libs");
    let space = root.join("sales");
    std::fs::create_dir_all(&space).unwrap();
    populate_space(&space);
    let out = root.join("out");

    let res = bin()
        .arg("--json")
        .arg("build")
        .arg(&space)
        .arg(&out)
        .arg("--shared-libs")
        .output()
        .unwrap();

    assert!(res.status.success());
    let v: serde_json::Value = serde_json::from_slice(&res.stdout).unwrap();
    assert_eq!(v["mode"], "shared-libs");
    assert_eq!(v["base_libs_bundled"], false);

    // The wrapped page keeps the absolute server path; no libs are bundled.
    let home = std::fs::read_to_string(out.join("index.html")).unwrap();
    assert!(home.contains(r#"href="/_gp/v1/base.css""#));
    assert!(!out.join("_gp").exists());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn non_index_home_emits_a_redirect_index() {
    let root = temp_dir("home-redirect");
    let space = root.join("docs");
    std::fs::create_dir_all(&space).unwrap();
    // No `index` artifact; a manifest picks the home, but even without one the
    // scanner resolves `home` > first-in-order. Use a single non-index artifact.
    write(&space, "overview.html", b"<h1>Overview</h1>");
    let out = root.join("out");

    let res = bin()
        .arg("--json")
        .arg("build")
        .arg(&space)
        .arg(&out)
        .output()
        .unwrap();
    assert!(res.status.success());

    // index.html is a redirect to the resolved home page.
    let idx = std::fs::read_to_string(out.join("index.html")).unwrap();
    assert!(idx.contains("url=overview.html"), "idx: {idx}");
    assert!(out.join("overview.html").is_file());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn reserved_slug_is_refused_like_the_server_path() {
    let root = temp_dir("reserved");
    let space = root.join("sales");
    std::fs::create_dir_all(&space).unwrap();
    write(&space, "api.html", b"<h1>x</h1>"); // `api` is reserved
    let out = root.join("out");

    let res = bin()
        .arg("--json")
        .arg("build")
        .arg(&space)
        .arg(&out)
        .output()
        .unwrap();

    assert_eq!(res.status.code(), Some(1));
    assert!(res.stdout.is_empty(), "stdout should be empty on error");
    let err: serde_json::Value = serde_json::from_slice(&res.stderr).unwrap();
    assert_eq!(err["error"]["code"], "reserved_slug");
    // Nothing was written — the refusal happens before any output.
    assert!(!out.exists(), "no output on a refused scan");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn output_inside_the_source_space_is_refused() {
    let root = temp_dir("out-in-space");
    let space = root.join("sales");
    std::fs::create_dir_all(&space).unwrap();
    populate_space(&space);
    // out nested inside the source space → refused (would pollute the next scan).
    let out = space.join("dist");

    let res = bin()
        .arg("--json")
        .arg("build")
        .arg(&space)
        .arg(&out)
        .output()
        .unwrap();

    assert_eq!(res.status.code(), Some(1));
    let err: serde_json::Value = serde_json::from_slice(&res.stderr).unwrap();
    assert_eq!(err["error"]["code"], "output_inside_space");
    assert!(!out.exists(), "nothing written when output is refused");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn non_empty_output_requires_force() {
    let root = temp_dir("clobber");
    let space = root.join("sales");
    std::fs::create_dir_all(&space).unwrap();
    populate_space(&space);
    let out = root.join("out");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("keep.txt"), b"existing").unwrap();

    // Without --force → refused.
    let res = bin()
        .arg("--json")
        .arg("build")
        .arg(&space)
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(res.status.code(), Some(1));
    let err: serde_json::Value = serde_json::from_slice(&res.stderr).unwrap();
    assert_eq!(err["error"]["code"], "output_not_empty");
    // The pre-existing file is untouched.
    assert_eq!(std::fs::read(out.join("keep.txt")).unwrap(), b"existing");

    // With --force → succeeds, writing alongside the existing file.
    let res2 = bin()
        .arg("build")
        .arg(&space)
        .arg(&out)
        .arg("--force")
        .output()
        .unwrap();
    assert!(res2.status.success());
    assert!(out.join("index.html").is_file());
    assert!(out.join("keep.txt").is_file());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn dry_run_plans_without_writing() {
    let root = temp_dir("dry-run");
    let space = root.join("sales");
    std::fs::create_dir_all(&space).unwrap();
    populate_space(&space);
    let out = root.join("out");

    let res = bin()
        .arg("--json")
        .arg("build")
        .arg(&space)
        .arg(&out)
        .arg("--dry-run")
        .output()
        .unwrap();

    assert!(res.status.success());
    let v: serde_json::Value = serde_json::from_slice(&res.stdout).unwrap();
    assert_eq!(v["dry_run"], true);
    // The planning envelope lists what WOULD be written…
    let would = v["would"].as_array().unwrap();
    assert!(would.iter().any(|w| w["path"] == "index.html"));
    assert!(would.iter().any(|w| w["path"] == "_gp/v1/base.css"));
    // …but nothing was actually written.
    assert!(
        !out.exists(),
        "dry-run must not create the output directory"
    );

    let _ = std::fs::remove_dir_all(&root);
}
