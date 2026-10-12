use camel_audio_runtime::language::{CompiledProgram, Engine, ExecutionEvent, Host};
use std::sync::{Arc, Mutex};

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
    assert!(matches!(&events[1],ExecutionEvent::Log{text} if text=="before"));
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
