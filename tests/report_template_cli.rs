use std::process::Command;

#[test]
fn build_report_builtin_via_cli() {
    let root = std::env::temp_dir().join(format!("glasspad-report-cli-{}", std::process::id()));
    let space = root.join("reportspace");
    std::fs::create_dir_all(&space).unwrap();
    std::fs::write(space.join("glasspad.yaml"), "template: report\n").unwrap();
    std::fs::write(
        space.join("index.md"),
        "# Report\n\nDate.\n\n## Summary\n\nText.\n",
    )
    .unwrap();
    let out = root.join("out");
    let output = Command::new(env!("CARGO_BIN_EXE_glasspad"))
        .args(["--json", "build"])
        .arg(&space)
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["built"], true);
    let html = std::fs::read_to_string(out.join("index.html")).unwrap();
    assert!(html.contains("<article class=\"gp-report\">"));
    assert!(html.contains("/_gp/v1/base.css") || html.contains("_gp/v1/base.css"));
    assert!(!html.contains("gp-toc"));
    let _ = std::fs::remove_dir_all(root);
}
