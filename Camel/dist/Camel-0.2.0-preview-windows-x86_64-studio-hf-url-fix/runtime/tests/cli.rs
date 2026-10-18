use std::process::Command;
fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .into()
}
#[test]
fn cli_runs_general_program_and_emits_only_valid_json() {
    let r = Command::new(env!("CARGO_BIN_EXE_camel"))
        .current_dir(root())
        .args(["Camel/examples/programming_core.camel", "--json", "--", "6"])
        .output()
        .unwrap();
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let json: serde_json::Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(json["value"]["fibonacci"], 8);
    assert_eq!(json["value"]["sumOfSquares"], 91);
    assert_eq!(json["logs"].as_array().unwrap().len(), 1);
}
#[test]
fn check_validates_audio_source_without_model_setup() {
    let r = Command::new(env!("CARGO_BIN_EXE_camel"))
        .current_dir(root())
        .env("CAMEL_LM_ENDPOINT", "invalid-and-unreachable")
        .args(["check", "Camel/examples/audio_note_summarizer.camel"])
        .output()
        .unwrap();
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
}
#[test]
fn cli_type_failure_exits_with_source_diagnostic() {
    let r = Command::new(env!("CARGO_BIN_EXE_camel"))
        .current_dir(root())
        .args(["Camel/examples/programming_core.camel", "--", "-1"])
        .output()
        .unwrap();
    assert!(!r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("Count must be 0..50"));
}

#[test]
fn cli_prints_each_independent_analysis_diagnostic() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("multiple-errors.camel");
    std::fs::write(
        &source,
        "workflow int First() { return \"bad\"; }\nworkflow bool Second() { return 1; }",
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_camel"))
        .current_dir(root())
        .arg(source)
        .output()
        .unwrap();

    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert_eq!(stderr.matches("CAMEL-TYPE-001").count(), 2, "{stderr}");
}
