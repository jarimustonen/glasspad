//! Build script: stamp the short git commit SHA of the build into the binary.
//!
//! `glasspad version --json` reports `data.commit`. This script resolves the
//! short SHA at COMPILE time and emits it under an **internal-only** carrier
//! env var, `GLASSPAD_COMMIT`, which `cli::version` reads with `option_env!`.
//! It is deliberately best-effort and **fail-open** — a missing SHA never fails
//! the build:
//!
//! * The SHA is read from **this crate's own repository only**: git is anchored
//!   with `-C $CARGO_MANIFEST_DIR` and we first require a `.git` entry to exist
//!   there (glasspad is the repo-root crate). Without it we emit nothing — so a
//!   crates.io / `cargo install` tarball (which has no `.git`) reports `null`,
//!   and we never walk *up* into an unrelated ancestor repository and stamp its
//!   SHA. `.git` is a directory in a normal checkout and a file in a linked
//!   worktree; both satisfy the `exists()` gate and git handles both.
//! * The value is validated as a lowercase-hex short SHA before it is emitted,
//!   so `data.commit` is never a bogus or partial hash, and a hostile/garbled
//!   value can never inject extra `cargo:` directives into this script's output.
//! * `GLASSPAD_BUILD_COMMIT` in the environment is an authoritative **override
//!   input** (CI provenance / reproducible builds), consumed and validated here:
//!   a valid hex value is honored; an empty or invalid value disables stamping
//!   (reports `null`); either way git is not consulted. Crucially, the carrier
//!   `cli::version` reads is the *distinct* `GLASSPAD_COMMIT` — which only this
//!   script ever sets — so a user-set `GLASSPAD_BUILD_COMMIT` cannot flow into
//!   `data.commit` unvalidated (a bare `option_env!` reads the ambient compile
//!   environment, so reading the public override name directly would bypass the
//!   validation below).
//!
//! The reported `commit` is the repository **HEAD** at build time — a dirty
//! (uncommitted) tree still reports its HEAD, not a guarantee the binary matches
//! that commit byte-for-byte. No dependencies, no `vergen`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The internal, build-script-only carrier env var `cli::version` reads. Kept
/// distinct from the public `GLASSPAD_BUILD_COMMIT` override input so that only
/// a value this script validated can reach `data.commit`.
const CARRIER: &str = "GLASSPAD_COMMIT";

fn main() {
    // An explicit env override wins over git and is authoritative. An empty or
    // invalid value is an explicit "no stamp" (→ `null`); git is not consulted.
    println!("cargo:rerun-if-env-changed=GLASSPAD_BUILD_COMMIT");
    if let Ok(raw) = std::env::var("GLASSPAD_BUILD_COMMIT") {
        if let Some(sha) = valid_short_sha(&raw) {
            println!("cargo:rustc-env={CARRIER}={sha}");
        }
        // else: emit nothing → `option_env!` is `None` → `data.commit` is `null`.
        return;
    }

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Only stamp from *this* crate's repo. No `.git` here (crates.io tarball) →
    // no stamp, and no walking up into an unrelated ancestor repository.
    if !manifest.join(".git").exists() {
        return;
    }

    emit_rerun_watches(&manifest);

    if let Some(sha) = git_short_sha(&manifest) {
        println!("cargo:rustc-env={CARRIER}={sha}");
    }
    // Else: emit nothing. `option_env!("GLASSPAD_COMMIT")` → `None` → `null`.
}

/// A short git SHA of `HEAD` for the repository at `manifest`, or `None` when
/// git is unavailable, this is not a git checkout, or the output is not a
/// well-formed short SHA. Never panics. `--short=12` pins the abbreviation
/// length so the emitted value is stable across machines regardless of the
/// developer's `core.abbrev`; `HEAD^{commit}` ensures we resolve a commit.
fn git_short_sha(manifest: &Path) -> Option<String> {
    let out = git(
        manifest,
        &["rev-parse", "--verify", "--short=12", "HEAD^{commit}"],
    )?;
    valid_short_sha(&out)
}

/// Tell cargo to re-run this script whenever HEAD, the branch ref HEAD points
/// at, or `packed-refs` changes — so an ordinary commit (which updates the ref
/// file, **not** `HEAD`) re-stamps the SHA. Paths are resolved through git
/// (`--git-path`), which returns the correct locations for a linked worktree
/// where `.git` is a pointer file and the real refs live in the common gitdir.
fn emit_rerun_watches(manifest: &Path) {
    if let Some(head) = git(manifest, &["rev-parse", "--git-path", "HEAD"]) {
        println!("cargo:rerun-if-changed={head}");
    }
    // The loose ref HEAD points at (attached branch). Detached HEAD has no
    // symbolic ref — its movement is caught by the HEAD watch above.
    if let Some(reference) = git(manifest, &["symbolic-ref", "-q", "HEAD"])
        && let Some(ref_path) = git(manifest, &["rev-parse", "--git-path", &reference])
    {
        println!("cargo:rerun-if-changed={ref_path}");
    }
    // The ref may be packed rather than loose.
    if let Some(packed) = git(manifest, &["rev-parse", "--git-path", "packed-refs"]) {
        println!("cargo:rerun-if-changed={packed}");
    }
}

/// Run `git -C <manifest> <args>` and return trimmed stdout on a clean exit, or
/// `None` on any failure (git missing, non-zero exit, non-UTF-8). `GIT_DIR` /
/// `GIT_WORK_TREE` are cleared so an inherited environment cannot redirect
/// discovery to an unrelated repository. Never panics.
fn git(manifest: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(manifest)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Validate a candidate commit stamp: a non-empty, all-lowercase-hex string of
/// 7..=64 chars (a short SHA up to a full SHA-256 id). Returns the normalized
/// value on success, `None` otherwise. This gate is why `data.commit` is never
/// a partial/bogus hash and why an env override can never smuggle a newline (and
/// thus a spurious `cargo:` directive) into the build-script output.
fn valid_short_sha(candidate: &str) -> Option<String> {
    let s = candidate.trim();
    let ok = (7..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    ok.then(|| s.to_string())
}
