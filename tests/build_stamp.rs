//! Contract test for the crate-root `build.rs` commit stamp — the crates.io /
//! tarball case: **a build with no `.git` present must succeed and leave
//! `GLASSPAD_BUILD_COMMIT` unset** (so `data.commit` reports `null`, never a
//! failed build or a bogus hash).
//!
//! Rather than mutate this repo's own `.git` (destructive, and it *is* a
//! checkout), we build a throwaway zero-dependency crate in a system temp dir —
//! outside any git work tree — whose `build.rs` IS this crate's `build.rs`
//! verbatim, and whose `main` prints `option_env!("GLASSPAD_BUILD_COMMIT")`.
//! Because the temp crate is not inside a checkout, `git rev-parse` fails, the
//! build script emits no env var, and the program prints `null`. The build
//! succeeding at all is the primary assertion.

use std::process::Command;

#[test]
fn build_script_falls_back_to_null_without_git() {
    let cargo = env!("CARGO");
    let our_build_rs = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/build.rs"))
        .expect("read this crate's build.rs");

    // A unique temp dir under the OS temp root, which is not a git work tree.
    let root = std::env::temp_dir().join(format!(
        "glasspad-stamp-probe-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    // Guard against a temp root that is itself inside a checkout — the whole test
    // is meaningless if `git rev-parse` would succeed here.
    assert!(
        !Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .current_dir(std::env::temp_dir())
            .output()
            .map(|o| o.status.success() && o.stdout.starts_with(b"true"))
            .unwrap_or(false),
        "temp dir is inside a git checkout; cannot exercise the no-.git fallback"
    );

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        // Zero dependencies → offline, fast build. Own target dir keeps it isolated.
        "[package]\nname = \"stamp_probe\"\nversion = \"0.0.0\"\nedition = \"2021\"\nbuild = \"build.rs\"\n\n[[bin]]\nname = \"stamp_probe\"\npath = \"src/main.rs\"\n",
    )
    .unwrap();
    std::fs::write(root.join("build.rs"), &our_build_rs).unwrap();
    std::fs::write(
        root.join("src/main.rs"),
        "fn main() { match option_env!(\"GLASSPAD_BUILD_COMMIT\") { Some(s) => println!(\"{s}\"), None => println!(\"null\") } }\n",
    )
    .unwrap();

    let out = Command::new(cargo)
        .args(["run", "--quiet"])
        .current_dir(&root)
        // Do not let an inherited override defeat the no-git fallback we're testing.
        .env_remove("GLASSPAD_BUILD_COMMIT")
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
