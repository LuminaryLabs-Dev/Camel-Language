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
fn library_lock_command_creates_a_reproducible_lock_that_verifies_offline() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = root().join("libraries/index.json");
    let first = dir.path().join("first.lock");
    let second = dir.path().join("second.lock");
    for output in [&first, &second] {
        let result = Command::new(env!("CARGO_BIN_EXE_camel"))
            .current_dir(root())
            .args(["library", "lock", "standard", "audio", "--catalog"])
            .arg(&catalog)
            .arg("--output")
            .arg(output)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    assert_eq!(
        std::fs::read(&first).unwrap(),
        std::fs::read(&second).unwrap()
    );
    let verified = Command::new(env!("CARGO_BIN_EXE_camel"))
        .current_dir(root())
        .arg("library")
        .arg("verify-lock")
        .arg(&first)
        .output()
        .unwrap();
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
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

#[test]
fn cli_blocks_model_calls_without_explicit_run_approval() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("approval.camel");
    std::fs::write(
        &source,
        "agent A{model=\"local\";} workflow Main(){Agent a=A; await a.run(\"prompt\");}",
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_camel"))
        .current_dir(root())
        .env("CAMEL_LM_ENDPOINT", "http://127.0.0.1:1")
        .arg(source)
        .output()
        .unwrap();

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr)
        .contains("approval required for capability 'model'"));
}

#[test]
fn replay_evaluation_reports_are_byte_stable() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("evaluate.camel");
    let cassette = dir.path().join("evaluate.cassette.json");
    let report = dir.path().join("evaluate.report.json");
    std::fs::write(&source, "workflow int Main() { return 7; }").unwrap();

    let recorded = Command::new(env!("CARGO_BIN_EXE_camel"))
        .current_dir(root())
        .arg(&source)
        .arg("--record-cassette")
        .arg(&cassette)
        .output()
        .unwrap();
    assert!(
        recorded.status.success(),
        "{}",
        String::from_utf8_lossy(&recorded.stderr)
    );

    let run_report = || {
        let output = Command::new(env!("CARGO_BIN_EXE_camel"))
            .current_dir(root())
            .arg(&source)
            .arg("--replay-cassette")
            .arg(&cassette)
            .arg("--evaluation-report")
            .arg(&report)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::read(&report).unwrap()
    };
    let first = run_report();
    let second = run_report();
    assert_eq!(first, second);
    let report: serde_json::Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(report["format"], "camel-evaluation-v1");
    assert_eq!(report["result"], 7);
    assert!(report.get("elapsedMs").is_none());
}
