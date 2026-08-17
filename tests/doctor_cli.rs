//! End-to-end tests for the read-only `glasspad doctor` self-diagnostic.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_glasspad"))
}

fn tmp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("gp-doctor-{tag}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

fn hermetic(cwd: &Path, home: &Path) -> Command {
    let mut command = bin();
    command
        .current_dir(cwd)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env_remove("GLASSPAD_TARGET")
        .env_remove("GLASSPAD_SERVER")
        .env_remove("GLASSPAD_API_KEY");
    command
}

fn tree_snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        let mut entries: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap())
            .collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            if entry.file_type().unwrap().is_dir() {
                out.push((relative.clone(), Vec::new()));
                walk(root, &path, out);
            } else {
                out.push((relative, fs::read(path).unwrap()));
            }
        }
    }

    let mut snapshot = Vec::new();
    walk(root, root, &mut snapshot);
    snapshot
}

#[test]
fn doctor_all_green_is_read_only() {
    let cwd = tmp_dir("green-cwd");
    let home = tmp_dir("green-home");
    write(&cwd.join("sentinel.txt"), "unchanged");
    write(&home.join("sentinel.txt"), "unchanged");
    let before_cwd = tree_snapshot(&cwd);
    let before_home = tree_snapshot(&home);

    let out = hermetic(&cwd, &home)
        .args(["doctor", "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(
        value["summary"],
        serde_json::json!({"ok": 3, "warn": 0, "fail": 0})
    );
    let checks = value["checks"].as_array().unwrap();
    assert_eq!(checks.len(), 3);
    for check in checks {
        assert!(check.get("id").is_some());
        assert_eq!(check["status"], "ok");
        assert!(check.get("message").is_some());
        assert!(check.get("fix_suggestion").is_some());
    }

    assert_eq!(
        tree_snapshot(&cwd),
        before_cwd,
        "doctor modified the cwd tree"
    );
    assert_eq!(
        tree_snapshot(&home),
        before_home,
        "doctor modified the home tree"
    );
}

#[test]
fn doctor_failure_exits_one_with_canonical_json_shape() {
    let cwd = tmp_dir("fail-cwd");
    let home = tmp_dir("fail-home");
    write(
        &home.join("glasspad/config.yaml"),
        "target: [not valid here\n",
    );

    let out = hermetic(&cwd, &home)
        .args(["doctor", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert!(value["summary"]["fail"].as_u64().unwrap() >= 1);
    for check in value["checks"].as_array().unwrap() {
        assert!(check["id"].is_string());
        assert!(matches!(
            check["status"].as_str(),
            Some("ok" | "warn" | "fail")
        ));
        assert!(check["message"].is_string());
        assert!(check.get("fix_suggestion").is_some());
    }
    let config = value["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["id"] == "config.file")
        .unwrap();
    assert_eq!(config["status"], "fail");
    assert!(config["fix_suggestion"].is_string());
}

#[test]
fn doctor_never_prints_api_key_material() {
    let cwd = tmp_dir("secret-cwd");
    let home = tmp_dir("secret-home");
    let secret = "doctor-secret-must-never-appear";
    write(
        &home.join("glasspad/config.yaml"),
        &format!("target: hosted\nserver: https://example.invalid\napi_key: {secret}\n"),
    );

    for args in [["doctor", "--json"], ["doctor", ""]] {
        let actual_args: Vec<_> = args.into_iter().filter(|arg| !arg.is_empty()).collect();
        let out = hermetic(&cwd, &home).args(actual_args).output().unwrap();
        assert!(out.status.success());
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!combined.contains(secret), "secret leaked: {combined}");
        assert!(
            combined.contains("<set>"),
            "redacted state missing: {combined}"
        );
    }
}
