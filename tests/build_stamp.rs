//! Contract test for the crate-root `build.rs` commit stamp — the crates.io /
//! tarball case: **a build with no `.git` at the crate root must succeed and
//! leave `GLASSPAD_BUILD_COMMIT` unset** (so `data.commit` reports `null`, never
//! a failed build or a bogus/ancestor hash).
//!
//! Rather than mutate this repo's own `.git` (destructive, and it *is* a
//! checkout), we build a throwaway zero-dependency crate in a system temp dir
//! whose `build.rs` IS this crate's `build.rs` verbatim, and whose `main` prints
//! `option_env!("GLASSPAD_BUILD_COMMIT")`. The probe crate has **no `.git` at
//! its own manifest dir**, so `build.rs`'s repo-root gate short-circuits and it
//! never consults git — which is precisely the crates.io-tarball path. Because
//! the gate keys off the crate's *own* `.git` (not an upward `git rev-parse`
//! walk), the result is `null` even if the temp dir happens to sit under some
//! ancestor repository. Git discovery is further fenced off (`GIT_CEILING_*`,
//! cleared `GIT_*`) so the outcome cannot depend on the runner's layout, and the
//! build is `--offline` with an isolated `--target-dir` for hermeticity.

use std::process::Command;

#[test]
fn build_script_falls_back_to_null_without_git() {
    let cargo = env!("CARGO");
    let our_build_rs = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/build.rs"))
        .expect("read this crate's build.rs");

    // A unique temp dir under the OS temp root.
    let root = std::env::temp_dir().join(format!(
        "glasspad-stamp-probe-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        // Zero dependencies → offline-buildable and fast. Edition 2024 matches
        // the real crate so `build.rs` (which uses edition-2024 let-chains)
        // compiles here exactly as it does in the parent build.
        "[package]\nname = \"stamp_probe\"\nversion = \"0.0.0\"\nedition = \"2024\"\nbuild = \"build.rs\"\n\n[[bin]]\nname = \"stamp_probe\"\npath = \"src/main.rs\"\n",
    )
    .unwrap();
    std::fs::write(root.join("build.rs"), &our_build_rs).unwrap();
    std::fs::write(
        root.join("src/main.rs"),
        // Reads the same internal carrier `cli::version` reads (the value
        // `build.rs` emits), not the public `GLASSPAD_BUILD_COMMIT` override.
        "fn main() { match option_env!(\"GLASSPAD_COMMIT\") { Some(s) => println!(\"{s}\"), None => println!(\"null\") } }\n",
    )
    .unwrap();

    // `git rev-parse` walks upward, so fence its discovery to the temp tree even
    // though `build.rs`'s repo-root gate already prevents an ancestor stamp — a
    // belt-and-braces guarantee the fallback is exercised, not accidentally
    // defeated by the runner's directory layout or an inherited git env.
    let out = Command::new(cargo)
        .args(["run", "--quiet", "--offline"])
        .arg("--target-dir")
        .arg(root.join("target"))
        .current_dir(&root)
        // Clear BOTH the public override input and the internal carrier: cargo
        // exposes this crate's own `cargo:rustc-env=GLASSPAD_COMMIT=<sha>` in the
        // test process's environment, which the inner build would otherwise
        // inherit and `option_env!` would read — a false pass masking the gate.
        .env_remove("GLASSPAD_BUILD_COMMIT")
        .env_remove("GLASSPAD_COMMIT")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env("GIT_CEILING_DIRECTORIES", &root)
        .output()
        .expect("run the probe crate");

    // Primary assertion: the build did not fail despite the absent `.git`.
    assert!(
        out.status.success(),
        "probe build/run failed:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    // And the stamp is absent → the program falls back to `null`.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout.trim(),
        "null",
        "no-.git build must leave GLASSPAD_BUILD_COMMIT unset; got: {stdout:?}"
    );

    let _ = std::fs::remove_dir_all(&root);
}
