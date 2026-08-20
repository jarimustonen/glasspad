//! End-to-end contract tests for the canonical `glasspad skill` subcommands and
//! the backward-compatible `skill --install-claude [--json]` surface.
//!
//! Drives the built binary (`CARGO_BIN_EXE_glasspad`) so the tests exercise the
//! real CLI surface: under `--json` the install emits ONLY a versioned envelope
//! on stdout (success) or a structured error on stderr (missing `.claude/`),
//! while the non-`--json` path keeps its human "Installed skill to …" line.
//! Project scope resolves `.claude/` relative to the process cwd, so each test
//! runs the binary in a throwaway temp directory.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_glasspad"))
}

/// Create (and return) a uniquely-named empty temp directory for one test.
/// Uses the process id + a caller-supplied tag to avoid collisions without any
/// randomness source.
fn temp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("glasspad-skill-test-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn frontmatter_field<'a>(content: &'a str, key: &str) -> &'a str {
    let frontmatter = content
        .strip_prefix("---\n")
        .and_then(|body| body.split_once("\n---\n"))
        .map(|(frontmatter, _)| frontmatter)
        .expect("skill content must start with YAML frontmatter");
    frontmatter
        .lines()
        .find_map(|line| {
            let (field, value) = line.split_once(':')?;
            (field == key).then(|| value.trim().trim_matches('"'))
        })
        .unwrap_or_else(|| panic!("skill frontmatter must contain {key}"))
}

#[test]
fn skill_list_json_agrees_with_version_metadata() {
    let list_out = bin().args(["skill", "list", "--json"]).output().unwrap();
    assert!(list_out.status.success(), "stderr: {:?}", list_out.stderr);
    assert!(list_out.stderr.is_empty());
    let list: serde_json::Value = serde_json::from_slice(&list_out.stdout).unwrap();
    assert_eq!(list["schema_version"], 1);
    assert_eq!(list["warnings"], serde_json::json!([]));
    let listed = list["data"]["skills"].as_array().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["name"], "glasspad");
    assert_eq!(listed[0]["cli_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(listed[0]["schema_version"], 1);
    let content = include_str!("../crates/glasspad-cli/src/skill.md");
    assert_eq!(listed[0]["name"], frontmatter_field(content, "name"));
    assert_eq!(
        listed[0]["description"],
        frontmatter_field(content, "description")
    );
    assert_eq!(
        listed[0]["cli_version"],
        frontmatter_field(content, "cli_version")
    );
    assert_eq!(
        listed[0]["schema_version"].to_string(),
        frontmatter_field(content, "schema_version")
    );

    let version_out = bin().args(["version", "--json"]).output().unwrap();
    assert!(version_out.status.success());
    let version: serde_json::Value = serde_json::from_slice(&version_out.stdout).unwrap();
    let listed_metadata: Vec<_> = listed
        .iter()
        .map(|skill| {
            serde_json::json!({
                "name": skill["name"],
                "cli_version": skill["cli_version"],
                "schema_version": skill["schema_version"],
            })
        })
        .collect();
    assert_eq!(
        version["data"]["skills"],
        serde_json::json!(listed_metadata)
    );
}

#[test]
fn skill_print_streams_exact_content_without_side_effects() {
    let root = temp_dir("print-read-only");
    let out = bin()
        .current_dir(&root)
        .args(["skill", "print", "glasspad"])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    assert_eq!(
        out.stdout,
        include_bytes!("../crates/glasspad-cli/src/skill.md")
    );
    assert!(out.stderr.is_empty());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);

    let json_out = bin()
        .current_dir(&root)
        .args(["skill", "print", "glasspad", "--json"])
        .output()
        .unwrap();
    assert!(json_out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&json_out.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["name"], "glasspad");
    assert_eq!(value["cli_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(value["schema_version_skill"], 1);
    assert_eq!(
        value["content"],
        include_str!("../crates/glasspad-cli/src/skill.md")
    );
    assert_eq!(value["path_in_repo"], "crates/glasspad-cli/src/skill.md");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn skill_print_unknown_name_is_structured_caller_error() {
    let out = bin()
        .args(["skill", "print", "not-bundled", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    let error: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(error["schema_version"], 1);
    assert_eq!(error["error"]["code"], "skill_not_found");
    assert_eq!(error["error"]["invalid_value"], "not-bundled");
    assert_eq!(error["error"]["expected"], serde_json::json!(["glasspad"]));
}

#[test]
fn canonical_no_name_install_covers_the_listed_inventory() {
    let root = temp_dir("canonical-install-all");
    let list_out = bin().args(["skill", "list", "--json"]).output().unwrap();
    let list: serde_json::Value = serde_json::from_slice(&list_out.stdout).unwrap();
    let listed_names: Vec<&str> = list["data"]["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|skill| skill["name"].as_str().unwrap())
        .collect();

    let out = bin()
        .current_dir(&root)
        .args(["skill", "install", "--agent", "pi", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let installed_names: Vec<&str> = value["targets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|target| target["skill"].as_str().unwrap())
        .collect();
    assert_eq!(installed_names, listed_names);
    for name in listed_names {
        assert!(
            root.join(".pi/skills")
                .join(name)
                .join("SKILL.md")
                .is_file()
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn canonical_skill_install_routes_to_existing_installer() {
    let root = temp_dir("canonical-install");
    let out = bin()
        .current_dir(&root)
        .args(["skill", "install", "glasspad", "--agent", "pi", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["installed"], true);
    assert_eq!(value["targets"][0]["agent"], "pi");
    assert_eq!(
        std::fs::read_to_string(root.join(".pi/skills/glasspad/SKILL.md")).unwrap(),
        include_str!("../crates/glasspad-cli/src/skill.md")
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn canonical_install_unknown_name_is_structured_caller_error() {
    let out = bin()
        .args(["skill", "install", "not-bundled", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    let error: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(error["error"]["code"], "skill_not_found");
    assert_eq!(error["error"]["invalid_value"], "not-bundled");
}

#[test]
fn bare_skill_compatibility_remains_a_raw_dump() {
    for args in [&["skill"][..], &["skill", "--json"][..]] {
        let out = bin().args(args).output().unwrap();
        assert!(out.status.success(), "stderr: {:?}", out.stderr);
        assert_eq!(
            out.stdout,
            include_bytes!("../crates/glasspad-cli/src/skill.md")
        );
        assert!(out.stderr.is_empty());
    }
}

#[test]
fn compatibility_install_flag_conflicts_with_subcommands() {
    for flag in ["--install", "--install-claude"] {
        let out = bin()
            .args(["--json", "skill", flag, "list"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1), "flag: {flag}");
        assert!(out.stdout.is_empty(), "flag: {flag}");
        let error: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(error["error"]["code"], "invalid_skill_invocation");
        assert_eq!(
            error["error"]["invalid_value"],
            "compatibility install flag"
        );
    }
}

#[test]
fn project_install_json_envelope_shape() {
    let root = temp_dir("proj-json");
    std::fs::create_dir_all(root.join(".claude")).unwrap();

    let out = bin()
        .current_dir(&root)
        .arg("--json")
        .arg("skill")
        .arg("--install-claude")
        .output()
        .unwrap();

    assert!(out.status.success(), "exit: {:?}", out.status);
    // stdout is ONLY the JSON envelope — no plain-text "Installed skill to …" line.
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["installed"], true);
    assert_eq!(v["scope"], "project");
    assert_eq!(v["created"], true);
    assert_eq!(v["cli_version"], env!("CARGO_PKG_VERSION"));
    // `warnings` is present (empty) for cross-command uniformity.
    assert_eq!(v["warnings"], serde_json::json!([]));
    // Top-level path/created describe the Claude target (backward-compatible).
    let path = v["path"].as_str().unwrap();
    assert!(
        path.ends_with(".claude/skills/glasspad/SKILL.md"),
        "path: {path}"
    );
    // The installed file has the real skill content, not an empty/partial write.
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        include_str!("../crates/glasspad-cli/src/skill.md")
    );
    // Dual-home default (--agent all): both Claude and pi targets are reported and
    // written. The pi project target lands under ./.pi/skills/glasspad/.
    let targets = v["targets"].as_array().unwrap();
    assert_eq!(targets.len(), 2, "targets: {targets:?}");
    assert_eq!(targets[0]["agent"], "claude");
    assert_eq!(targets[0]["created"], true);
    assert_eq!(targets[1]["agent"], "pi");
    assert_eq!(targets[1]["created"], true);
    let pi_path = targets[1]["path"].as_str().unwrap();
    assert!(
        pi_path.ends_with(".pi/skills/glasspad/SKILL.md"),
        "pi_path: {pi_path}"
    );
    assert_eq!(
        std::fs::read_to_string(pi_path).unwrap(),
        include_str!("../crates/glasspad-cli/src/skill.md")
    );
    assert!(out.stderr.is_empty(), "stderr: {:?}", out.stderr);

    // A second install into the same tree reports created=false (overwritten),
    // and still lands the correct content.
    let out2 = bin()
        .current_dir(&root)
        .arg("--json")
        .arg("skill")
        .arg("--install-claude")
        .output()
        .unwrap();
    let v2: serde_json::Value = serde_json::from_slice(&out2.stdout).unwrap();
    assert_eq!(v2["created"], false);
    assert_eq!(
        std::fs::read_to_string(v2["path"].as_str().unwrap()).unwrap(),
        include_str!("../crates/glasspad-cli/src/skill.md")
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn user_install_json_envelope() {
    // `--user` resolves `.claude/` under $HOME; point HOME (and USERPROFILE, the
    // Windows equivalent `dirs` consults) at a throwaway dir so the test never
    // touches the real user home.
    let home = temp_dir("user-home");

    let out = bin()
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .arg("--json")
        .arg("skill")
        .arg("--install-claude")
        .arg("--user")
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "stderr: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["scope"], "user");
    assert_eq!(v["created"], true);
    assert_eq!(v["warnings"], serde_json::json!([]));
    let path = v["path"].as_str().unwrap();
    assert!(path.ends_with("skills/glasspad/SKILL.md"), "path: {path}");
    // The install landed under our overridden HOME, not the real one.
    assert!(
        std::path::Path::new(path).starts_with(std::fs::canonicalize(&home).unwrap()),
        "path {path} escaped the test HOME {}",
        home.display()
    );
    assert!(out.stderr.is_empty(), "stderr: {:?}", out.stderr);

    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn project_install_text_dual_home_lines() {
    let root = temp_dir("proj-text");
    std::fs::create_dir_all(root.join(".claude")).unwrap();

    let out = bin()
        .current_dir(&root)
        .arg("skill")
        .arg("--install-claude")
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    // The plain-text success path prints one "Installed skill to …" line per
    // dual-homed target — Claude first, then pi.
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "stdout: {stdout}");
    assert!(
        lines[0].starts_with("Installed skill to ") && lines[0].contains(".claude/"),
        "line 0: {}",
        lines[0]
    );
    assert!(
        lines[1].starts_with("Installed skill to ") && lines[1].contains(".pi/"),
        "line 1: {}",
        lines[1]
    );
    assert!(out.stderr.is_empty(), "stderr: {:?}", out.stderr);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn claude_only_agent_writes_no_pi_target() {
    // `--agent claude` preserves the pre-dual-home behavior: exactly one target,
    // the Claude dir; no ./.pi/ tree is created.
    let root = temp_dir("proj-claude-only");
    std::fs::create_dir_all(root.join(".claude")).unwrap();

    let out = bin()
        .current_dir(&root)
        .arg("--json")
        .arg("skill")
        .arg("--install-claude")
        .arg("--agent")
        .arg("claude")
        .output()
        .unwrap();

    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let targets = v["targets"].as_array().unwrap();
    assert_eq!(targets.len(), 1, "targets: {targets:?}");
    assert_eq!(targets[0]["agent"], "claude");
    assert!(v["path"].as_str().unwrap().contains(".claude/"));
    // No pi tree was created.
    assert!(!root.join(".pi").exists(), "unexpected .pi/ created");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn pi_user_install_writes_pi_dir() {
    // `--agent pi --user` is the migration case: the skill lands under
    // ~/.pi/agent/skills/glasspad/SKILL.md and NOT under ~/.claude/.
    let home = temp_dir("pi-user-home");

    let out = bin()
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .arg("--json")
        .arg("skill")
        .arg("--install-claude")
        .arg("--user")
        .arg("--agent")
        .arg("pi")
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "stderr: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["scope"], "user");
    let targets = v["targets"].as_array().unwrap();
    assert_eq!(targets.len(), 1, "targets: {targets:?}");
    assert_eq!(targets[0]["agent"], "pi");
    // Top-level path (first target) describes pi when Claude is not selected.
    let path = v["path"].as_str().unwrap();
    assert!(
        path.ends_with(".pi/agent/skills/glasspad/SKILL.md"),
        "path: {path}"
    );
    assert!(
        std::path::Path::new(path).starts_with(std::fs::canonicalize(&home).unwrap()),
        "path {path} escaped the test HOME {}",
        home.display()
    );
    // The Claude dir was NOT touched.
    assert!(
        !home.join(".claude").exists(),
        "unexpected ~/.claude created for --agent pi"
    );
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        include_str!("../crates/glasspad-cli/src/skill.md")
    );

    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn dual_home_user_install_writes_both_dirs() {
    // Default `--agent all --user`: both ~/.claude and ~/.pi/agent get the skill,
    // and re-running is idempotent (created flips to false, content stays intact).
    let home = temp_dir("dual-user-home");

    let run = || {
        bin()
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .arg("--json")
            .arg("skill")
            .arg("--install-claude")
            .arg("--user")
            .output()
            .unwrap()
    };

    let out = run();
    assert!(
        out.status.success(),
        "stderr: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let targets = v["targets"].as_array().unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0]["agent"], "claude");
    assert_eq!(targets[1]["agent"], "pi");
    let claude_path = home.join(".claude/skills/glasspad/SKILL.md");
    let pi_path = home.join(".pi/agent/skills/glasspad/SKILL.md");
    assert!(claude_path.exists(), "claude skill missing");
    assert!(pi_path.exists(), "pi skill missing");
    assert_eq!(
        std::fs::read_to_string(&pi_path).unwrap(),
        include_str!("../crates/glasspad-cli/src/skill.md")
    );

    // Idempotent re-run: safe, and both targets report created=false.
    let out2 = run();
    assert!(out2.status.success());
    let v2: serde_json::Value = serde_json::from_slice(&out2.stdout).unwrap();
    for t in v2["targets"].as_array().unwrap() {
        assert_eq!(t["created"], false, "re-run should overwrite, not create");
    }
    assert_eq!(
        std::fs::read_to_string(&pi_path).unwrap(),
        include_str!("../crates/glasspad-cli/src/skill.md")
    );

    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn pi_project_install_writes_pi_dir_only() {
    // `--agent pi` at project scope creates ./.pi/skills/glasspad/ on demand (no
    // .claude/ guard needed) and never touches a Claude dir. Top-level path/created
    // describe the pi target since Claude is not selected.
    let root = temp_dir("pi-proj");

    let out = bin()
        .current_dir(&root)
        .arg("--json")
        .arg("skill")
        .arg("--install-claude")
        .arg("--agent")
        .arg("pi")
        .output()
        .unwrap();

    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["scope"], "project");
    let path = v["path"].as_str().unwrap();
    assert!(
        path.ends_with(".pi/skills/glasspad/SKILL.md"),
        "path: {path}"
    );
    let targets = v["targets"].as_array().unwrap();
    assert_eq!(targets.len(), 1, "targets: {targets:?}");
    assert_eq!(targets[0]["agent"], "pi");
    assert_eq!(targets[0]["created"], true);
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        include_str!("../crates/glasspad-cli/src/skill.md")
    );
    // No Claude tree was created.
    assert!(
        !root.join(".claude").exists(),
        "unexpected .claude/ created"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn install_refuses_to_overwrite_a_symlinked_skill() {
    // A pre-planted `SKILL.md` symlink must NOT be followed and truncated with the
    // skill content (CWE-59); the install refuses with a structured error, exit 1,
    // and leaves the symlink's target untouched.
    let root = temp_dir("symlink");
    let skill_dir = root.join(".claude/skills/glasspad");
    std::fs::create_dir_all(&skill_dir).unwrap();
    // A sensitive file the symlink points at; it must survive untouched.
    let secret = root.join("secret.txt");
    std::fs::write(&secret, "SECRET").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&secret, skill_dir.join("SKILL.md")).unwrap();

    #[cfg(unix)]
    {
        let out = bin()
            .current_dir(&root)
            .arg("--json")
            .arg("skill")
            .arg("--install-claude")
            .arg("--agent")
            .arg("claude")
            .output()
            .unwrap();

        assert_eq!(out.status.code(), Some(1), "stdout: {:?}", out.stdout);
        assert!(out.stdout.is_empty(), "stdout should be empty on error");
        let err: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(err["error"]["code"], "symlink_rejected");
        // The symlink target was NOT overwritten with skill content.
        assert_eq!(std::fs::read_to_string(&secret).unwrap(), "SECRET");
    }

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn agent_without_install_is_a_usage_error() {
    // `--agent` requires `--install-claude` (clap `requires`); passing it alone must
    // be a usage error (exit 2), not a silent skill-content dump.
    let root = temp_dir("agent-no-install");

    let out = bin()
        .current_dir(&root)
        .arg("skill")
        .arg("--agent")
        .arg("pi")
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(2), "stdout: {:?}", out.stdout);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn install_alias_matches_install_claude() {
    // `--install` is the preferred spelling; it must behave identically to the
    // `--install-claude` compatibility alias.
    let root = temp_dir("install-alias");
    std::fs::create_dir_all(root.join(".claude")).unwrap();

    let out = bin()
        .current_dir(&root)
        .arg("--json")
        .arg("skill")
        .arg("--install")
        .arg("--agent")
        .arg("claude")
        .output()
        .unwrap();

    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["installed"], true);
    assert_eq!(v["targets"].as_array().unwrap()[0]["agent"], "claude");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn project_install_missing_claude_dir_json_error() {
    // No `.claude/` in the cwd → structured error on stderr, exit 1, empty stdout.
    let root = temp_dir("proj-missing");

    let out = bin()
        .current_dir(&root)
        .arg("--json")
        .arg("skill")
        .arg("--install-claude")
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty(), "stdout should be empty on error");
    let err: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(err["schema_version"], 1);
    assert_eq!(err["error"]["code"], "claude_dir_not_found");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn project_install_missing_claude_dir_text_error() {
    // The non-`--json` error path stays a human `error:` line on stderr, exit 1.
    let root = temp_dir("proj-missing-text");

    let out = bin()
        .current_dir(&root)
        .arg("skill")
        .arg("--install-claude")
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.starts_with("error: "), "stderr: {stderr}");

    let _ = std::fs::remove_dir_all(&root);
}
