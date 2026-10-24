use camel_audio_runtime::language::{
    analyze_buffer, analyze_file, analyze_source, CompiledProgram, Engine, ExecutionEvent, Host,
    Severity,
};
use std::sync::{Arc, Mutex};

#[test]
fn file_buffer_and_source_analysis_return_matching_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("parity.camel");
    std::fs::write(&path, "").unwrap();
    let path = path.canonicalize().unwrap();
    let file_name = path.display().to_string();

    for source in [
        "workflow void Main() { int first = \"bad\"; int second = false; }",
        "workflow void Main(int unused) {}",
    ] {
        std::fs::write(&path, source).unwrap();
        let from_file = analyze_file(&path);
        let from_buffer = analyze_buffer(&path, source);
        let from_source = analyze_source(&file_name, source);

        let signature = |result: &camel_audio_runtime::language::AnalysisResult| {
            result
                .diagnostics()
                .iter()
                .map(|diagnostic| {
                    let span = &diagnostic.primary_span;
                    (
                        format!("{:?}", diagnostic.severity),
                        format!("{:?}", diagnostic.phase),
                        diagnostic.code.clone(),
                        diagnostic.message.clone(),
                        span.start.line,
                        span.start.column,
                        span.start.offset,
                        span.end.line,
                        span.end.column,
                        span.end.offset,
                    )
                })
                .collect::<Vec<_>>()
        };
        let expected = signature(&from_file);
        assert_eq!(signature(&from_buffer), expected);
        assert_eq!(signature(&from_source), expected);
        assert_eq!(from_file.is_valid(), from_buffer.is_valid());
        assert_eq!(from_file.is_valid(), from_source.is_valid());
    }
}

#[test]
fn imported_file_diagnostics_keep_the_imported_source_span_across_entry_modes() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("main.camel");
    let imported = dir.path().join("support.camel");
    let entry_source = "import \"support.camel\"; workflow int Main() { return Helper(); }";
    let imported_source = "int Helper() { int invalid = \"wrong\"; return 1; }";
    std::fs::write(&entry, entry_source).unwrap();
    std::fs::write(&imported, imported_source).unwrap();
    let entry = entry.canonicalize().unwrap();
    let imported = imported.canonicalize().unwrap();

    let file = analyze_file(&entry);
    let buffer = analyze_buffer(&entry, entry_source);
    let diagnostic = file
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code == "CAMEL-TYPE-001")
        .expect("imported type diagnostic");
    assert_eq!(
        diagnostic.primary_span.start.file,
        imported.display().to_string()
    );
    assert_eq!(diagnostic.primary_span.start.line, 1);
    assert_eq!(
        diagnostic.primary_span.end.offset,
        imported_source.find("\"wrong\"").unwrap() + 7
    );
    let signature = |result: &camel_audio_runtime::language::AnalysisResult| {
        result
            .diagnostics()
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.code.clone(),
                    diagnostic.message.clone(),
                    diagnostic.primary_span.start.file.clone(),
                    diagnostic.primary_span.start.line,
                    diagnostic.primary_span.start.column,
                    diagnostic.primary_span.start.offset,
                    diagnostic.primary_span.end.line,
                    diagnostic.primary_span.end.column,
                    diagnostic.primary_span.end.offset,
                    diagnostic.phase,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(signature(&buffer), signature(&file));
}

#[test]
fn analyze_source_returns_program_metadata_and_structured_diagnostic() {
    let valid = analyze_source("good.camel", "workflow int Main() { return 7; }");
    assert!(valid.is_valid());
    assert!(valid.program().is_some());
    assert!(valid.diagnostics().is_empty());
    assert!(valid.declarations().iter().any(|d| d.name == "Main"));

    let invalid = analyze_source("bad.camel", "workflow int Main() { return \"x\"; }");
    assert!(!invalid.is_valid());
    assert!(invalid.program().is_none());
    assert_eq!(invalid.diagnostics().len(), 1);
    assert_eq!(invalid.diagnostics()[0].severity, Severity::Error);
    assert_eq!(invalid.diagnostics()[0].code, "CAMEL-TYPE-001");
    assert_eq!(
        invalid.diagnostics()[0].primary_span.start.file,
        "bad.camel"
    );
    assert_eq!(invalid.diagnostics()[0].primary_span.start.line, 1);
    assert!(invalid.diagnostics()[0].related.is_empty());
    assert!(invalid.diagnostics()[0].help.is_none());
}

#[test]
fn analysis_collects_independent_type_diagnostics_in_source_order() {
    let source = concat!(
        "workflow void Main() {\n",
        "  int first = \"bad\";\n",
        "  int second = false;\n",
        "}\n",
        "workflow int Other() { return \"also bad\"; }"
    );
    let result = analyze_source("multiple-errors.camel", source);

    assert!(!result.is_valid());
    assert_eq!(result.diagnostics().len(), 3);
    assert!(result
        .diagnostics()
        .iter()
        .all(|diagnostic| diagnostic.code == "CAMEL-TYPE-001"));
    assert!(
        result
            .diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.phase
                == camel_audio_runtime::language::DiagnosticPhase::Type)
    );
    assert_eq!(result.diagnostics()[0].primary_span.start.line, 2);
    assert_eq!(result.diagnostics()[1].primary_span.start.line, 3);
    assert_eq!(result.diagnostics()[2].primary_span.start.line, 5);
}

#[test]
fn lint_resolves_shadowed_names_to_their_own_scopes() {
    let source = concat!(
        "workflow void Main() {\n",
        "  int item = 1;\n",
        "  {\n",
        "    int item = 2;\n",
        "    log(item);\n",
        "  }\n",
        "}"
    );
    let result = analyze_source("shadowed.camel", source);

    assert!(result.is_valid());
    assert_eq!(result.diagnostics().len(), 1);
    assert_eq!(result.diagnostics()[0].code, "CAMEL-LINT-001");
    assert_eq!(
        result.diagnostics()[0].message,
        "local 'item' is never used"
    );
    assert_eq!(result.diagnostics()[0].primary_span.start.line, 2);
}

#[test]
fn lint_does_not_count_a_write_as_a_read() {
    let result = analyze_source(
        "write-only.camel",
        "workflow void Main() { int value = 1; value = 2; }",
    );

    assert!(result.is_valid());
    assert_eq!(result.diagnostics().len(), 1);
    assert_eq!(
        result.diagnostics()[0].message,
        "local 'value' is never used"
    );
}

#[test]
fn lint_warnings_keep_program_runnable_and_check_codes_ignore_wording() {
    let with_warning = analyze_source("warning.camel", "workflow void Main(int unused) {}");
    assert!(with_warning.is_valid());
    assert!(with_warning.program().is_some());
    assert_eq!(with_warning.diagnostics()[0].code, "CAMEL-LINT-002");

    let errors = analyze_source(
        "stable-codes.camel",
        concat!(
            "workflow void Main() { return 1; }\n",
            "int Helper() { return missing; }"
        ),
    );
    assert_eq!(errors.diagnostics().len(), 2);
    assert!(errors
        .diagnostics()
        .iter()
        .all(|diagnostic| diagnostic.code == "CAMEL-CHECK-001"));
    assert!(errors
        .diagnostics()
        .iter()
        .all(|diagnostic| diagnostic.phase
            == camel_audio_runtime::language::DiagnosticPhase::Analysis));
}

#[test]
fn unknown_type_has_an_explicit_type_diagnostic_code() {
    let result = analyze_source(
        "unknown-type.camel",
        "workflow void Main() { Missing value = 1; }",
    );

    assert_eq!(result.diagnostics().len(), 1);
    assert_eq!(result.diagnostics()[0].code, "CAMEL-TYPE-002");
    assert_eq!(
        result.diagnostics()[0].phase,
        camel_audio_runtime::language::DiagnosticPhase::Type
    );
}

#[test]
fn analysis_keeps_first_error_compatibility_for_program_loaders() {
    let result = analyze_source(
        "bad-syntax.camel",
        "workflow int First( { return 1; } workflow int Second( { return 2; }",
    );

    assert_eq!(result.diagnostics().len(), 1);
    assert_eq!(result.diagnostics()[0].code, "CAMEL-SYNTAX-001");
    let error = match result.into_program() {
        Err(error) => error,
        Ok(_) => panic!("invalid source unexpectedly produced a program"),
    };
    assert_eq!(error.site.file, "bad-syntax.camel");
}

#[test]
fn checking_unsaved_buffers_resolves_imports_without_running_defaults() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("math.camel"),
        "int Twice(int x) { return x * 2; }",
    )
    .unwrap();
    let source = "import \"math.camel\"; workflow int Main(int x = 3) { return Twice(x); }";
    let p = CompiledProgram::from_source_at(dir.path().join("unsaved.camel"), source).unwrap();
    let declarations = p.declarations();
    let main = declarations.iter().find(|d| d.name == "Main").unwrap();
    assert_eq!(main.kind, "workflow");
    assert_eq!(main.parameters[0].default.as_deref(), Some("3"));
    assert!(!dir.path().join("unsaved.camel").exists());
    let args = p.parse_arguments("Main", &["7".into()]).unwrap();
    assert_eq!(
        Engine::new(p, Host::default())
            .run("Main", args)
            .unwrap()
            .value
            .display(),
        "14"
    );
}
#[test]
fn live_events_survive_errors_and_observer_panics_do_not_break_execution() {
    let p = CompiledProgram::from_source(
        "test.camel",
        "workflow void Main() { log(\"before\"); throw \"bad\"; }",
    )
    .unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let host = Host {
        observer: Some(Arc::new(move |e| sink.lock().unwrap().push(e))),
        ..Host::default()
    };
    assert!(Engine::new(p.clone(), host).run("Main", vec![]).is_err());
    let events = events.lock().unwrap();
    assert!(matches!(events.first(),Some(ExecutionEvent::Started{kind,..}) if kind=="workflow"));
    assert!(events
        .iter()
        .any(|event| matches!(event, ExecutionEvent::Log { text } if text == "before")));
    assert!(events
        .iter()
        .any(|event| matches!(event, ExecutionEvent::Budget { .. })));
    assert!(matches!(
        events.last(),
        Some(ExecutionEvent::Finished { success: false, .. })
    ));
    drop(events);
    let p =
        CompiledProgram::from_source("test.camel", "workflow int Main() { return 4; }").unwrap();
    let host = Host {
        observer: Some(Arc::new(|_| panic!("observer"))),
        ..Host::default()
    };
    assert_eq!(
        Engine::new(p, host)
            .run("Main", vec![])
            .unwrap()
            .value
            .display(),
        "4"
    );
}
#[test]
fn inspector_does_not_evaluate_agent_configuration() {
    let source="string Fail() { throw \"must not run\"; } agent A { model = Fail(); } workflow void Main() {}";
    let p = CompiledProgram::from_source("test.camel", source).unwrap();
    let a = p
        .declarations()
        .into_iter()
        .find(|d| d.name == "A")
        .unwrap();
    assert_eq!(a.details, vec![("model".into(), "Fail()".into())]);
}
