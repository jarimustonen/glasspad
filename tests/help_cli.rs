//! End-to-end contract tests for AI-first `--help --json` (§14).

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_glasspad"))
}

fn help(args: &[&str]) -> serde_json::Value {
    let output = bin().args(args).output().expect("run glasspad");
    assert!(output.status.success(), "exit: {:?}", output.status);
    assert!(
        output.stderr.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON help envelope")
}

#[test]
fn top_level_help_json_uses_the_shared_envelope() {
    let value = help(&["--help", "--json"]);
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["data"]["schema_version_help"], 3);
    assert_eq!(value["data"]["command"], "glasspad");
    assert_eq!(value["warnings"], serde_json::json!([]));
    assert!(
        value["data"]["subcommands"]
            .as_array()
            .is_some_and(|v| !v.is_empty())
    );
    assert!(
        value["data"]["examples"]
            .as_array()
            .is_some_and(|v| !v.is_empty())
    );
}

#[test]
fn nested_help_drills_down_with_flags_examples_and_env() {
    let value = help(&["publish", "--help", "--json"]);
    assert_eq!(value["data"]["command"], "glasspad publish");
    let flags = value["data"]["flags"].as_array().expect("flags");
    let flag = |name: &str| {
        flags
            .iter()
            .find(|flag| flag["name"] == name)
            .unwrap_or_else(|| panic!("missing flag {name}"))
    };
    assert_eq!(flag("port")["short"], "p");
    assert_eq!(flag("port")["long"], "port");
    assert_eq!(flag("port")["env"], "GLASSPAD_PORT");
    assert_eq!(flag("api_key")["env"], "GLASSPAD_API_KEY");
    assert_eq!(flag("api_key")["defaults"], serde_json::json!([]));
    assert!(
        value["data"]["examples"]
            .as_array()
            .is_some_and(|v| !v.is_empty())
    );
}

#[test]
fn api_key_values_never_reach_help_output() {
    let output = bin()
        .args([
            "publish",
            "--api-key",
            "ARGV_API_KEY_SENTINEL",
            "--help",
            "--json",
        ])
        .env("GLASSPAD_API_KEY", "ENV_API_KEY_SENTINEL")
        .output()
        .expect("run glasspad");
    assert!(output.status.success(), "exit: {:?}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    for sentinel in ["ARGV_API_KEY_SENTINEL", "ENV_API_KEY_SENTINEL"] {
        assert!(!stdout.contains(sentinel), "secret leaked to stdout");
        assert!(!stderr.contains(sentinel), "secret leaked to stderr");
    }
}

#[test]
fn text_help_remains_clap_text() {
    let output = bin().args(["publish", "--help"]).output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Usage:"));
    assert!(!stdout.trim_start().starts_with('{'));
}

#[test]
fn accepted_values_defaults_and_positionals_are_derived() {
    let data = help(&["data", "--help", "--json"]);
    let positional = &data["data"]["positionals"][0];
    assert_eq!(positional["name"], "file");
    assert_eq!(positional["required"], true);
    assert_eq!(positional["accepts_file_paths"], true);
    let format = data["data"]["flags"]
        .as_array()
        .unwrap()
        .iter()
        .find(|flag| flag["name"] == "format")
        .expect("format flag");
    assert_eq!(
        format["accepted_values"],
        serde_json::json!(["csv", "json", "mbox"])
    );

    let host = help(&["host-serve", "--help", "--json"]);
    let retention = host["data"]["flags"]
        .as_array()
        .unwrap()
        .iter()
        .find(|flag| flag["name"] == "retention_days")
        .expect("retention-days flag");
    assert_eq!(retention["defaults"], serde_json::json!(["90"]));
}

#[cfg(unix)]
#[test]
fn structured_help_accepts_non_utf8_argv() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let invalid_path = OsString::from_vec(vec![b'.', b'/', 0xff]);
    let output = bin()
        .arg("publish")
        .arg(invalid_path)
        .args(["--help", "--json"])
        .output()
        .expect("run glasspad");
    assert!(output.status.success(), "exit: {:?}", output.status);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["command"], "glasspad publish");
}

#[test]
fn nested_noun_help_lists_its_children() {
    let value = help(&["skill", "--json", "--help"]);
    let commands: Vec<_> = value["data"]["subcommands"]
        .as_array()
        .expect("subcommands")
        .iter()
        .map(|entry| entry["command"].as_str().expect("command"))
        .collect();
    assert_eq!(
        commands,
        [
            "glasspad skill help",
            "glasspad skill install",
            "glasspad skill list",
            "glasspad skill print"
        ]
    );
}

#[test]
fn generated_help_subcommand_is_also_drillable() {
    let value = help(&["help", "publish", "--help", "--json"]);
    assert_eq!(value["data"]["command"], "glasspad help publish");
    assert_eq!(value["data"]["flags"], serde_json::json!([]));
    assert!(
        value["data"]["examples"]
            .as_array()
            .is_some_and(|examples| !examples.is_empty())
    );
}
