//! End-to-end contract tests for `glasspad data` (Wave 5 / Phase 6).
//!
//! Drives the built binary (`CARGO_BIN_EXE_glasspad`) so the tests exercise the
//! real CLI surface: stdout = data channel, stderr = human/error envelope, and
//! the stable exit codes (1 = user error, 2 = system). The `data` helper never
//! starts a server, so these are fast and need no port.

use std::io::Write;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_glasspad"))
}

/// Write `content` to a uniquely-named temp file with `ext`, returning its path.
/// Uses the process id + a caller-supplied tag to avoid collisions without any
/// randomness source.
fn temp_file(tag: &str, ext: &str, content: &[u8]) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "glasspad-data-test-{}-{}.{ext}",
        std::process::id(),
        tag
    ));
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(content).unwrap();
    p
}

#[test]
fn csv_text_mode_emits_rows_on_stdout() {
    let path = temp_file("csv-text", "csv", b"name,age\nAda,36\nGrace,45\n");
    let out = bin().arg("data").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert!(out.status.success(), "exit: {:?}", out.status);
    let stdout = String::from_utf8(out.stdout).unwrap();
    // Rows are JSON on stdout; the human summary goes to stderr.
    assert!(stdout.contains("\"name\""), "stdout: {stdout}");
    assert!(stdout.contains("Ada"), "stdout: {stdout}");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("parsed 2 rows"), "stderr: {stderr}");
}

#[test]
fn csv_json_envelope_shape() {
    let path = temp_file("csv-json", "csv", b"x\n1\n2\n3\n");
    let out = bin().arg("--json").arg("data").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["format"], "csv");
    assert_eq!(v["row_count"], 3);
    assert!(v["rows"].is_array());
    // `warnings` is present for cross-command uniformity.
    assert_eq!(v["warnings"], serde_json::json!([]));
    // `meta` only appears with --meta.
    assert!(v.get("meta").is_none());
}

#[test]
fn json_with_meta_infers_field_types() {
    let path = temp_file(
        "json-meta",
        "json",
        br#"[{"when":"2026-04-01","n":5},{"when":"2026-04-02","n":7}]"#,
    );
    let out = bin()
        .arg("--json")
        .arg("data")
        .arg(&path)
        .arg("--meta")
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);

    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["meta"]["fields"]["when"], "temporal");
    assert_eq!(v["meta"]["fields"]["n"], "number");
    assert_eq!(v["meta"]["row_count"], 2);
}

#[test]
fn format_override_on_extensionless_file() {
    let path = temp_file("noext-override", "", b"a,b\n1,2\n");
    let out = bin()
        .arg("data")
        .arg(&path)
        .arg("--format")
        .arg("csv")
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);

    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout).unwrap().contains("\"a\""));
}

#[test]
fn unknown_format_is_a_user_error_with_json_envelope() {
    // A `.txt` extension with no --format cannot be inferred → exit 1, structured
    // error on stderr (stdout stays the empty data channel).
    let path = temp_file("unknown", "txt", b"whatever");
    let out = bin().arg("--json").arg("data").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty(), "stdout should be empty on error");
    let err: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(err["error"]["code"], "unknown_format");
}

#[test]
fn missing_file_is_a_user_error() {
    let out = bin()
        .arg("--json")
        .arg("data")
        .arg("/no/such/glasspad/file.csv")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(err["error"]["code"], "no_such_path");
}

#[test]
fn invalid_utf8_json_uses_not_utf8_code() {
    // Invalid UTF-8 bytes with a .json extension → the stable `not_utf8` code,
    // not the generic `parse_failed`.
    let path = temp_file("bad-utf8", "json", &[0xff, 0xfe, 0x00]);
    let out = bin().arg("--json").arg("data").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(out.status.code(), Some(1));
    let err: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(err["error"]["code"], "not_utf8");
}

#[test]
fn directory_input_is_rejected() {
    // Force a format (a bare dir path has no extension → would hit unknown_format
    // first) so validation reaches the regular-file check.
    let dir = std::env::temp_dir();
    let out = bin()
        .arg("--json")
        .arg("data")
        .arg(&dir)
        .arg("--format")
        .arg("csv")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(err["error"]["code"], "not_a_file");
}
