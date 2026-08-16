//! End-to-end tests for the read-only `glasspad config` inspection surface.

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
    let dir = std::env::temp_dir().join(format!("gp-config-{tag}-{}-{nanos}", std::process::id()));
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
        .env_remove("GLASSPAD_SERVER")
        .env_remove("GLASSPAD_API_KEY")
        .env_remove("GLASSPAD_TARGET");
    command
}

#[test]
fn config_path_reports_missing_file_without_creating_it() {
    let cwd = tmp_dir("path-cwd");
    let home = tmp_dir("path-home");
    let expected = home.join("glasspad/config.yaml");

    let out = hermetic(&cwd, &home)
        .args(["config", "path"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("no config file exists"), "{stdout}");
    assert!(
        stdout.contains(expected.to_string_lossy().as_ref()),
        "{stdout}"
    );
    assert!(!expected.exists(), "config path must not create a file");
    assert!(out.stderr.is_empty());
}

#[test]
fn config_show_json_has_provenance_envelope_and_redacts_file_secret() {
    let cwd = tmp_dir("show-cwd");
    let home = tmp_dir("show-home");
    let config = home.join("glasspad/config.yaml");
    let secret = "file-secret-must-never-appear";
    write(
        &config,
        &format!("target: hosted\nserver: https://file.example\napi_key: {secret}\n"),
    );

    let out = hermetic(&cwd, &home)
        .args(["config", "show", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out.status);
    assert!(out.stderr.is_empty());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(!stdout.contains(secret), "secret leaked in JSON: {stdout}");
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["warnings"], serde_json::json!([]));
    assert_eq!(value["data"]["server"]["value"], "https://file.example");
    assert_eq!(value["data"]["server"]["source"], "config-file");
    assert_eq!(value["data"]["api_key"]["value"], "<set>");
    assert_eq!(value["data"]["api_key"]["source"], "config-file");
    assert_eq!(value["data"]["api_key"]["secret"], true);
    assert_eq!(value["data"]["target"]["value"], "hosted");
    assert_eq!(value["data"]["target"]["source"], "config-file");
}

#[test]
fn config_show_reports_flag_env_and_default_sources_and_never_leaks_secret() {
    let cwd = tmp_dir("sources-cwd");
    let home = tmp_dir("sources-home");
    let flag_secret = "flag-secret-must-never-appear";
    let out = hermetic(&cwd, &home)
        .args([
            "config",
            "show",
            "--server",
            "https://flag.example",
            "--api-key",
            flag_secret,
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        !stdout.contains(flag_secret),
        "secret leaked in text: {stdout}"
    );
    assert!(stdout.contains("server: https://flag.example (flag)"));
    assert!(stdout.contains("api_key: <set> (flag)"));

    let env_secret = "env-secret-must-never-appear";
    let out = hermetic(&cwd, &home)
        .env("GLASSPAD_SERVER", "https://env.example")
        .env("GLASSPAD_API_KEY", env_secret)
        .args(["config", "show", "--json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        !stdout.contains(env_secret),
        "secret leaked in JSON: {stdout}"
    );
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["data"]["server"]["source"], "env");
    assert_eq!(value["data"]["api_key"]["source"], "env");

    let out = hermetic(&cwd, &home)
        .args(["config", "show", "--json"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["data"]["server"]["value"], serde_json::Value::Null);
    assert_eq!(value["data"]["server"]["source"], "default");
    assert_eq!(value["data"]["api_key"]["value"], "<unset>");
    assert_eq!(value["data"]["api_key"]["source"], "default");
    assert_eq!(value["data"]["target"]["value"], "loopback");
    assert_eq!(value["data"]["target"]["source"], "default");
}
