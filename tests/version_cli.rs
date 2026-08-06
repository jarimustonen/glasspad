//! End-to-end contract tests for `glasspad version` and the `--version` / `-V`
//! flag.
//!
//! Drives the built binary (`CARGO_BIN_EXE_glasspad`) so the tests exercise the
//! real CLI surface: the fleet-updater version-gates installs off this output,
//! so the JSON envelope shape (`{schema_version, data: {name, version, commit},
//! warnings}` — nested under `data`, matching the sibling CLIs) and the
//! plain-text `glasspad <version>` line are a contract, not an accident. The
//! version reported must always be the compile-time `CARGO_PKG_VERSION`, and
//! every JSON spelling — `version --json`, `--json version`, `--json --version`,
//! `--version --json` — must yield the same envelope (the flag is not a
//! text-only clap built-in; it routes through the same code as the subcommand).

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_glasspad"))
}

/// Assert `out` is a successful, clean (`stderr` empty) run whose stdout is the
/// nested version envelope, and return the parsed JSON.
fn assert_version_envelope(out: &std::process::Output) -> serde_json::Value {
    assert!(out.status.success(), "exit: {:?}", out.status);
    assert!(out.stderr.is_empty(), "stderr: {:?}", out.stderr);
    // Exactly one line of output (the envelope), no trailing junk.
    assert_eq!(
        out.stdout.iter().filter(|&&b| b == b'\n').count(),
        1,
        "stdout should be exactly one line: {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema_version"], 1);
    // Payload is nested under `data`, matching orchestratectl/ossctl so the
    // fleet-updater reads `.data.version` uniformly across every tool.
    assert_eq!(v["data"]["name"], env!("CARGO_PKG_NAME"));
    assert_eq!(v["data"]["version"], env!("CARGO_PKG_VERSION"));
    // `commit` is always present (string when a release build injected it, else
    // null) so a strict consumer never hits a missing-field error.
    let commit = &v["data"]["commit"];
    assert!(
        commit.is_null() || commit.is_string(),
        "commit must be string or null: {commit:?}"
    );
    // `warnings` is present (empty) for cross-command uniformity.
    assert_eq!(v["warnings"], serde_json::json!([]));
    v
}

#[test]
fn version_subcommand_json_envelope() {
    assert_version_envelope(&bin().arg("version").arg("--json").output().unwrap());
}

/// Whether the crate is being tested from inside a git checkout — i.e. whether
/// `build.rs` had a `HEAD` to stamp. True in dev / the release gate; false when
/// tests run from a crates.io tarball (no `.git`). Used to decide whether
/// `commit` MUST be a real SHA or is legitimately `null`.
fn in_git_checkout() -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .map(|o| o.status.success() && o.stdout.starts_with(b"true"))
        .unwrap_or(false)
}

#[test]
fn commit_is_a_real_short_sha_in_a_git_build() {
    // The whole point of `build.rs`: when built inside a git checkout, `commit`
    // is the short SHA of `HEAD`, not `null`. When tests run outside a checkout
    // (crates.io tarball), the fallback is `null` — accepted there, since that
    // is exactly the contract `build_stamp.rs` verifies.
    let out = bin().arg("version").arg("--json").output().unwrap();
    let v = assert_version_envelope(&out);
    let commit = &v["data"]["commit"];
    if in_git_checkout() {
        let sha = commit
            .as_str()
            .unwrap_or_else(|| panic!("commit must be a SHA in a git build, got: {commit:?}"));
        // A `git rev-parse --short` SHA: lowercase hex, at least 7 chars.
        assert!(
            sha.len() >= 7
                && sha
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "commit must look like a short git SHA: {sha:?}"
        );
    } else {
        assert!(
            commit.is_null(),
            "outside a checkout commit must be null: {commit:?}"
        );
    }
}

#[test]
fn all_json_spellings_yield_the_same_envelope() {
    // `--json` is global and `-V/--version` routes through the same code as the
    // subcommand, so every spelling must produce byte-identical output. This is
    // the exact case a fleet-updater hits: appending `--json` to `--version`
    // must NOT fall back to plain text.
    let spellings: [&[&str]; 4] = [
        &["version", "--json"],
        &["--json", "version"],
        &["--json", "--version"],
        &["--version", "--json"],
    ];
    let mut outputs = spellings.iter().map(|args| {
        let out = bin().args(*args).output().unwrap();
        assert_version_envelope(&out);
        out.stdout
    });
    let first = outputs.next().unwrap();
    for (args, stdout) in spellings[1..].iter().zip(outputs) {
        assert_eq!(
            stdout,
            first,
            "`{}` diverged from `{}`",
            args.join(" "),
            spellings[0].join(" ")
        );
    }
}

#[test]
fn version_subcommand_text_line() {
    let out = bin().arg("version").output().unwrap();

    assert!(out.status.success(), "exit: {:?}", out.status);
    let stdout = String::from_utf8(out.stdout).unwrap();
    // Exactly the conventional one-line `<name> <version>` a `--version` prints.
    assert_eq!(
        stdout,
        format!("{} {}\n", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
    );
    assert!(out.stderr.is_empty(), "stderr: {:?}", out.stderr);
}

#[test]
fn version_flag_matches_subcommand_text() {
    // `--version` and `-V` print the same text line the `version` subcommand
    // does (no `--json`), so tooling can use whichever spelling it prefers.
    let expected = format!("{} {}\n", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    for flag in ["--version", "-V"] {
        let out = bin().arg(flag).output().unwrap();
        assert!(out.status.success(), "{flag} exit: {:?}", out.status);
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            expected,
            "flag: {flag}"
        );
        assert!(out.stderr.is_empty(), "{flag} stderr: {:?}", out.stderr);
    }
}
