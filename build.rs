//! Build script: stamp the short git commit SHA of the build into the binary.
//!
//! `glasspad version --json` reports `data.commit`. This script captures the
//! short SHA at COMPILE time and exposes it as `GLASSPAD_BUILD_COMMIT`, which
//! `cli::version` reads with `option_env!`. It is deliberately best-effort:
//!
//! * If `git` is missing, or the crate is built outside a git checkout (the
//!   `cargo publish` / crates.io tarball case, where there is no `.git`), we
//!   emit **no** env var. `option_env!` then returns `None` and `data.commit`
//!   stays `null` — the build never fails on account of a missing SHA.
//! * The var is only emitted on a clean `git rev-parse --short HEAD`, so a
//!   consumer never sees a bogus or partial hash.
//!
//! No dependencies, no `vergen` — a ~10-line shell-out is enough here.

use std::process::Command;

fn main() {
    // Re-run when HEAD moves so a rebuild picks up the new SHA. Best-effort:
    // in a normal checkout `.git/HEAD` is the ref that changes on commit/checkout.
    // A worktree's `.git` is a file (gitdir pointer); watching it still catches
    // the worktree being re-pointed, and a `cargo clean` build always re-stamps.
    println!("cargo:rerun-if-changed=.git/HEAD");
    // Allow an explicit override (e.g. CI injecting a known SHA) to force a re-run.
    println!("cargo:rerun-if-env-changed=GLASSPAD_BUILD_COMMIT");

    // If the SHA was injected via the environment, honor it verbatim and skip git.
    if let Ok(sha) = std::env::var("GLASSPAD_BUILD_COMMIT") {
        let sha = sha.trim();
        if !sha.is_empty() {
            println!("cargo:rustc-env=GLASSPAD_BUILD_COMMIT={sha}");
        }
        return;
    }

    if let Some(sha) = git_short_sha() {
        println!("cargo:rustc-env=GLASSPAD_BUILD_COMMIT={sha}");
    }
    // Else: emit nothing. `option_env!("GLASSPAD_BUILD_COMMIT")` → `None` → `null`.
}

/// The short git SHA of `HEAD`, or `None` when git is unavailable, this is not a
/// git checkout, or the command fails for any reason. Never panics.
fn git_short_sha() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8(output.stdout).ok()?;
    let sha = sha.trim();
    if sha.is_empty() {
        None
    } else {
        Some(sha.to_string())
    }
}
