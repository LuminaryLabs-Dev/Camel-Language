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
fn check_validates_agent_and_audio_sources_without_model_setup() {
    for example in [
        "Camel/examples/agent_planner.camel",
        "Camel/examples/audio_note_summarizer.camel",
    ] {
        let r = Command::new(env!("CARGO_BIN_EXE_camel"))
            .current_dir(root())
            .env("CAMEL_LM_ENDPOINT", "invalid-and-unreachable")
            .args(["check", example])
            .output()
            .unwrap();
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    }
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
