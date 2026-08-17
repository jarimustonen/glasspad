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
    assert!(!value.to_string().contains("api_key\":\""));
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
