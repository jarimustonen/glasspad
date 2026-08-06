//! End-to-end contract tests for `glasspad version` and the built-in
//! `--version` / `-V` flag.
//!
//! Drives the built binary (`CARGO_BIN_EXE_glasspad`) so the tests exercise the
//! real CLI surface: the fleet-updater version-gates installs off this output,
//! so the JSON envelope shape (`{schema_version, name, version, warnings}`) and
//! the plain-text `glasspad <version>` line are a contract, not an accident. The
//! version reported must always be the compile-time `CARGO_PKG_VERSION`.

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_glasspad"))
}

#[test]
fn version_subcommand_json_envelope() {
    let out = bin().arg("version").arg("--json").output().unwrap();

    assert!(out.status.success(), "exit: {:?}", out.status);
    // stdout (the data channel) is ONLY the JSON envelope; nothing on stderr.
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["name"], "glasspad");
    assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
    // `warnings` is present (empty) for cross-command uniformity.
    assert_eq!(v["warnings"], serde_json::json!([]));
    assert!(out.stderr.is_empty(), "stderr: {:?}", out.stderr);
}

#[test]
fn global_json_flag_before_subcommand_also_works() {
    // `--json` is a global flag, so `glasspad --json version` is equivalent to
    // `glasspad version --json` — both yield the same envelope on stdout.
    let out = bin().arg("--json").arg("version").output().unwrap();

    assert!(out.status.success(), "exit: {:?}", out.status);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
    assert!(out.stderr.is_empty(), "stderr: {:?}", out.stderr);
}

#[test]
fn version_subcommand_text_line() {
    let out = bin().arg("version").output().unwrap();

    assert!(out.status.success(), "exit: {:?}", out.status);
    let stdout = String::from_utf8(out.stdout).unwrap();
    // Exactly the same one line clap's built-in `--version` prints, so the two
    // entrypoints never drift: `glasspad <version>`.
    assert_eq!(stdout, format!("glasspad {}\n", env!("CARGO_PKG_VERSION")));
    assert!(out.stderr.is_empty(), "stderr: {:?}", out.stderr);
}

#[test]
fn builtin_version_flag_matches_subcommand() {
    // `--version` and `-V` are clap built-ins; both print the same text line the
    // `version` subcommand does, so tooling can use whichever it prefers.
    let expected = format!("glasspad {}\n", env!("CARGO_PKG_VERSION"));
    for flag in ["--version", "-V"] {
        let out = bin().arg(flag).output().unwrap();
        assert!(out.status.success(), "{flag} exit: {:?}", out.status);
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            expected,
            "flag: {flag}"
        );
    }
}
