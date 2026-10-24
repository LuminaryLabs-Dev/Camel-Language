use camel_audio_runtime::{
    language::{
        Capability, CompiledProgram, Engine, Host, Limits, ModelProvider, ReplayCassette, Value,
    },
    AudioInput,
};
use serde_json::{json, Value as Json};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

fn run(
    source: &str,
) -> camel_audio_runtime::language::Result<camel_audio_runtime::language::Execution> {
    Engine::new(
        CompiledProgram::from_source("test.camel", source)?,
        Host::default(),
    )
    .run("Main", vec![])
}
fn number(source: &str, expected: i64) {
    assert!(matches!(run(source).unwrap().value,Value::Int(n) if n==expected));
}
#[test]
fn recursive_functions_are_actual_source_execution() {
    number("int Fib(int n) { if (n < 2) return n; return Fib(n-1)+Fib(n-2); } workflow int Main() { return Fib(10); }",55);
}
#[test]
fn defaults_and_first_class_functions() {
    number("int Add(int a,int b=4) {return a+b;} workflow int Main() {Function operation=Add; return operation(3);}",7);
}
#[test]
fn loops_scopes_and_control_flow() {
    number("workflow int Main() {int total=0; for(int i=0;i<8;i++) {if(i==2) continue; if(i==6) break; total+=i;} foreach(int n in [2,3]) total+=n; int j=0; while(j<2) {int total=99;j++;} return total;}",18);
}
#[test]
fn records_arrays_interpolation_and_methods() {
    let result=run(r#"record Note {string title; int[] points;} workflow string Main() {Note n=new Note {title="Topic",points=[2,3]}; n.points[0]=4; return $"{n.title}: {n.points[0]+n.points[1]} {{ok}}";}"#).unwrap();
    assert!(matches!(result.value,Value::String(s) if s=="Topic: 7 {ok}"));
}
#[test]
fn short_circuit_avoids_throwing_calls() {
    number(
        "workflow int Main() {assert(false && int(\"bad\")==0 || true);return 9;}",
        9,
    );
}
#[test]
fn failed_record_mutation_does_not_commit() {
    number(
        r#"record Count {int value;} workflow int Main() {Count c=new Count {value=7};any wrong="wrong";try {c.value=wrong;} catch(e) {assert(e.message.contains("expected int"));} return c.value;}"#,
        7,
    );
}
#[test]
fn errors_and_finally_unwind() {
    number("workflow int Main() {int result=0;try {throw \"oops\";}catch(e) {assert(e.message==\"oops\");result=5;}finally {result+=2;}return result;}",7);
}
#[test]
fn syntax_unknown_names_types_and_arity_fail_during_check() {
    for source in [
        "workflow Main(){int n=;}",
        "workflow Main(){Missing(1);}",
        "workflow Main(){unknown x=1;}",
        "int F(int x){return x;} workflow Main(){F();}",
        "workflow Main(){break;}",
        "agent A {model=\"local\";typo=1;} workflow Main(){}",
    ] {
        assert!(
            CompiledProgram::from_source("broken.camel", source).is_err(),
            "{source}"
        );
    }
}
#[test]
fn check_never_records_or_calls_models() {
    CompiledProgram::from_source("test.camel","agent Anything {model=\"not-downloaded\";} workflow Main(){Audio a=record(5);Anything.run(a);}").unwrap();
}
#[test]
fn explicit_per_run_approval_blocks_unapproved_tool_and_model_effects() {
    let approvals = Some(std::collections::BTreeSet::new());
    let tool = CompiledProgram::from_source(
        "approval.camel",
        "tool string Fetch(string url){handler=\"http.get\";} workflow Main(){Fetch(\"https://example.invalid\");}",
    )
    .unwrap();
    let error = Engine::new(
        tool,
        Host {
            approved_capabilities: approvals.clone(),
            ..Host::default()
        },
    )
    .run("Main", vec![])
    .err()
    .unwrap();
    assert!(error
        .message
        .contains("approval required for capability 'network'"));

    let agent = CompiledProgram::from_source(
        "approval.camel",
        "agent A{model=\"local\";} workflow Main(){Agent a=A; await a.run(\"prompt\");}",
    )
    .unwrap();
    let error = Engine::new(
        agent,
        Host {
            approved_capabilities: approvals,
            ..Host::default()
        },
    )
    .run("Main", vec![])
    .err()
    .unwrap();
    assert!(error
        .message
        .contains("approval required for capability 'model'"));
}
#[test]
fn arithmetic_failures_have_source_locations() {
    for expression in ["1/0", "9223372036854775807+1", "[1][3]"] {
        let error = run(&format!("workflow int Main(){{return {expression};}}"))
            .err()
            .unwrap();
        assert_eq!(error.site.file, "test.camel");
        assert!(error.site.column > 1);
    }
}
#[test]
fn budgets_cannot_be_swallowed_by_catch() {
    let p = CompiledProgram::from_source(
        "fuel.camel",
        "workflow Main(){try{while(true){}}catch(e){log(\"ignored\");}} ",
    )
    .unwrap();
    let e = Engine::new(p, Host::default())
        .with_limits(Limits {
            steps: 50,
            ..Limits::default()
        })
        .run("Main", vec![])
        .err()
        .unwrap();
    assert!(e.fatal);
    assert!(e.message.contains("budget"));
}

#[test]
fn cancelled_run_stops_before_workflow_body() {
    let token = camel_audio_runtime::language::CancellationToken::default();
    token.cancel();
    let program =
        CompiledProgram::from_source("cancel.camel", "workflow Main(){log(\"ran\");}").unwrap();
    let host = Host {
        cancellation: token,
        ..Host::default()
    };
    let error = Engine::new(program, host)
        .run("Main", vec![])
        .err()
        .unwrap();
    assert!(error.fatal);
    assert!(error.message.contains("cancelled"));
}

#[test]
fn total_deadline_is_checked_before_workflow_body() {
    let program =
        CompiledProgram::from_source("deadline.camel", "workflow Main(){log(\"ran\");}").unwrap();
    let error = Engine::new(program, Host::default())
        .with_limits(Limits {
            deadline: std::time::Duration::ZERO,
            ..Limits::default()
        })
        .run("Main", vec![])
        .err()
        .unwrap();
    assert!(error.fatal);
    assert!(error.message.contains("deadline"));
}

#[test]
fn direct_tool_calls_use_run_wide_budget() {
    let program = CompiledProgram::from_source(
        "tool-budget.camel",
        "tool int Count(){handler=\"test.count\";} workflow Main(){Count();Count();}",
    )
    .unwrap();
    let mut host = Host::default();
    host.tools
        .register("test.count", Capability::Pure, |_, _, _| Ok(Value::Int(1)))
        .unwrap();
    let error = Engine::new(program, host)
        .with_limits(Limits {
            tool_calls: 1,
            ..Limits::default()
        })
        .run("Main", vec![])
        .err()
        .unwrap();
    assert!(error.fatal);
    assert!(error.message.contains("tool-call budget"));
}
#[test]
fn tasks_capture_values_and_memoize_await() {
    number("int Double(int n){return n*2;} workflow int Main(){int x=4;Task<int> t=spawn Double(x);x=99;int a=await t;return a+await t;}",16);
}
#[test]
fn tasks_drain_even_without_explicit_await() {
    let result = run("void Work(){log(\"finished\");} workflow Main(){spawn Work();}").unwrap();
    assert_eq!(result.logs, vec!["finished"]);
}

#[test]
fn caught_task_errors_are_not_rethrown_on_drain() {
    number("int Fail(){throw \"failed\";} workflow int Main(){var task=spawn Fail();try{await task;}catch(e){return 7;}return 0;}",7);
}
#[test]
fn task_result_types_are_checked_when_awaited() {
    let e =
        run("workflow Main(){Task<int> task=spawn json.parse(\"{\\\"value\\\":1}\");await task;}")
            .err()
            .unwrap();
    assert!(e.message.contains("expected int"));
}
#[test]
fn discarded_task_failures_are_not_silenced() {
    assert!(run("void Fail(){throw \"failed\";} workflow Main(){spawn Fail();}").is_err());
}
#[test]
fn compound_assignment_evaluates_index_once() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let mut host = Host::default();
    host.tools
        .register("test.index", Capability::Pure, move |_, _, _| {
            seen.fetch_add(1, Ordering::SeqCst);
            Ok(Value::Int(0))
        })
        .unwrap();
    let r=hosted("tool int Index(){handler=\"test.index\";}workflow int Main(){int[] a=[4];a[Index()]+=3;return a[0];}",host).unwrap();
    assert!(matches!(r.value, Value::Int(7)));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
#[test]
fn braces_inside_interpolation_strings_are_not_delimiters() {
    let result = run(r#"workflow string Main(){return $"{\"}\"}";}"#).unwrap();
    assert!(matches!(result.value,Value::String(s) if s=="}"));
}
#[test]
fn object_initializers_preserve_source_order() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let calls = seen.clone();
    let mut host = Host::default();
    host.tools
        .register("test.trace", Capability::Pure, move |a, _, _| {
            calls.lock().unwrap().push(a[0].display());
            Ok(a[0].clone())
        })
        .unwrap();
    hosted(r#"tool string Trace(string text){handler="test.trace";}workflow Main(){var o=new {z=Trace("z"),a=Trace("a")};}"#,host).unwrap();
    assert_eq!(*seen.lock().unwrap(), vec!["z", "a"]);
}
#[test]
fn excessive_parser_nesting_returns_an_error() {
    let source = format!("workflow bool Main(){{return {}true;}}", "!".repeat(200));
    assert!(CompiledProgram::from_source("deep.camel", &source).is_err());
}
#[test]
fn typed_void_tasks_are_supported() {
    let r = run("void Work(){log(\"done\");}workflow Main(){Task<void> t=spawn Work();await t;}")
        .unwrap();
    assert_eq!(r.logs, vec!["done"]);
}
#[derive(Default)]
struct Fake {
    requests: Mutex<Vec<Json>>,
    answers: Mutex<std::collections::VecDeque<std::result::Result<Json, String>>>,
}
impl ModelProvider for Fake {
    fn capabilities(&self) -> camel_audio_runtime::language::ProviderCapabilities {
        camel_audio_runtime::language::ProviderCapabilities {
            tool_calls: true,
            structured_output: true,
            token_usage: true,
        }
    }
    fn complete(&self, request: Json) -> std::result::Result<Json, String> {
        self.requests.lock().unwrap().push(request);
        let mut answer = self
            .answers
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected model request")?;
        answer["usage"] = json!({"total_tokens":1});
        Ok(answer)
    }
}
fn provider(answers: Vec<Json>) -> Arc<Fake> {
    Arc::new(Fake {
        answers: Mutex::new(answers.into_iter().map(Ok).collect()),
        ..Fake::default()
    })
}

#[test]
fn duplicate_tool_ids_never_repeat_handler_effects() {
    let call = json!({"content":null,"tool_calls":[{"id":"same","function":{"name":"Count","arguments":"{}"}}]});
    let fake = provider(vec![call.clone(), call]);
    let seen = Arc::new(AtomicUsize::new(0));
    let calls = seen.clone();
    let mut host = Host {
        provider: Some(fake),
        ..Host::default()
    };
    host.tools
        .register("test.once", Capability::Pure, move |_, _, _| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(Value::Int(1))
        })
        .unwrap();
    let e=hosted(r#"tool int Count(){handler="test.once";} policy P {allowTools=[Count];maxToolCalls=2;} agent A {model="m";tools=[Count];policy=P;maxTurns=3;} workflow Main(){A.run("p");}"#,host).err().unwrap();
    assert!(e.message.contains("duplicate"));
    assert_eq!(seen.load(Ordering::SeqCst), 1);
}
fn hosted(
    source: &str,
    host: Host,
) -> camel_audio_runtime::language::Result<camel_audio_runtime::language::Execution> {
    Engine::new(CompiledProgram::from_source("agents.camel", source)?, host).run("Main", vec![])
}

#[test]
fn checkpoint_resume_restores_scope_without_repeating_prior_effects() {
    let temp = tempfile::tempdir().unwrap();
    let checkpoint = temp.path().join("run.checkpoint.json");
    let calls = Arc::new(AtomicUsize::new(0));
    let source = concat!(
        "tool int Mark() { handler = \"test.mark\"; }\n",
        "workflow int Main() {\n",
        "  int before = Mark();\n",
        "  log(\"checkpointed\");\n",
        "  checkpoint(\"prepared\");\n",
        "  int after = Mark();\n",
        "  return before * 10 + after;\n",
        "}"
    );
    let program = CompiledProgram::from_source("checkpoint.camel", source).unwrap();
    let register = || {
        let calls = calls.clone();
        let mut host = Host::default();
        host.tools
            .register("test.mark", Capability::Pure, move |_, _, _| {
                Ok(Value::Int(calls.fetch_add(1, Ordering::SeqCst) as i64 + 1))
            })
            .unwrap();
        host
    };

    let first = Engine::new(program.clone(), register())
        .with_checkpoint_file(&checkpoint)
        .run("Main", vec![])
        .unwrap();
    assert!(matches!(first.value, Value::Int(12)));
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    let resumed = Engine::new(program, register())
        .resume_from(&checkpoint)
        .run("Main", vec![])
        .unwrap();
    assert!(matches!(resumed.value, Value::Int(13)));
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(resumed.logs, vec!["checkpointed"]);
}

#[test]
fn persisted_cassette_replays_model_without_a_provider_and_rejects_other_source() {
    let temp = tempfile::tempdir().unwrap();
    let cassette_path = temp.path().join("run.cassette.json");
    let source = "agent A { model = \"test-model\"; } workflow string Main() { return A.run(\"question\").text; }";
    let program = CompiledProgram::from_source("replay.camel", source).unwrap();
    let provider = provider(vec![json!({"content":"stable answer"})]);
    let recording = ReplayCassette::record_to(&cassette_path).unwrap();
    let recorded = Engine::new(
        program.clone(),
        Host {
            provider: Some(provider.clone()),
            cassette: Some(recording),
            ..Host::default()
        },
    )
    .run("Main", vec![])
    .unwrap();
    assert_eq!(provider.requests.lock().unwrap().len(), 1);

    let replay = ReplayCassette::replay_from(&cassette_path).unwrap();
    let replayed = Engine::new(
        program,
        Host {
            cassette: Some(replay),
            ..Host::default()
        },
    )
    .run("Main", vec![])
    .unwrap();
    assert_eq!(recorded.value.display(), "stable answer");
    assert_eq!(replayed.value.display(), recorded.value.display());
    assert_eq!(replayed.tokens_used, recorded.tokens_used);
    assert_eq!(replayed.cassette_fingerprint, recorded.cassette_fingerprint);
    assert_eq!(provider.requests.lock().unwrap().len(), 1);

    let changed = CompiledProgram::from_source(
        "replay.camel",
        &source.replace("question", "different question"),
    )
    .unwrap();
    let outcome = Engine::new(
        changed,
        Host {
            cassette: Some(ReplayCassette::replay_from(&cassette_path).unwrap()),
            ..Host::default()
        },
    )
    .run("Main", vec![]);
    let error = match outcome {
        Err(error) => error,
        Ok(_) => panic!("changed source unexpectedly replayed"),
    };
    assert!(error
        .message
        .contains("does not match this source fingerprint"));
}

#[test]
fn agent_result_handoff_keeps_typed_provenance() {
    let fake = provider(vec![
        json!({"content":"evidence"}),
        json!({"content":"received"}),
    ]);
    let source = concat!(
        "agent Researcher { model = \"research-model\"; }\n",
        "agent Reviewer { model = \"review-model\"; }\n",
        "workflow string Main() {\n",
        "  AgentResult evidence = await Researcher.run(\"collect\");\n",
        "  AgentResult review = await Reviewer.run(evidence);\n",
        "  return review.text;\n",
        "}"
    );
    let result = hosted(
        source,
        Host {
            provider: Some(fake.clone()),
            ..Host::default()
        },
    )
    .unwrap();
    assert_eq!(result.value.display(), "received");
    let requests = fake.requests.lock().unwrap();
    let handoff = requests[1]["messages"][1]["content"].as_str().unwrap();
    assert!(handoff.contains("\"provenance\""), "{handoff}");
    assert!(handoff.contains("\"agent\":\"Researcher\""), "{handoff}");
    assert!(
        handoff.contains("\"model\":\"research-model\""),
        "{handoff}"
    );
    assert!(handoff.contains("\"sourceFingerprint\""), "{handoff}");
}

#[test]
fn agent_name_model_system_and_return_type_come_from_source() {
    let fake = provider(vec![json!({"content":"{\"answer\":42}"})]);
    let result=hosted(r#"record Answer {int answer;} agent TotallyDifferentName {model="chosen-model";system="chosen instruction";response=Answer;} workflow int Main(){Agent worker=TotallyDifferentName;AgentResult result=await worker.run("question");return result.data.answer;}"#,Host {provider:Some(fake.clone()),..Host::default()}).unwrap();
    assert!(matches!(result.value, Value::Int(42)));
    let requests = fake.requests.lock().unwrap();
    assert_eq!(requests[0]["model"], "chosen-model");
    assert!(requests[0]["messages"][0]["content"]
        .as_str()
        .unwrap()
        .starts_with("chosen instruction"));
    assert_eq!(requests[0]["messages"][1]["content"], "question");
}
#[test]
fn typed_model_outputs_reject_missing_wrong_or_extra_fields() {
    for text in ["not json", "{}", "{\"n\":\"bad\"}", "{\"n\":4,\"extra\":0}"] {
        let fake = provider(vec![json!({"content":text})]);
        assert!(hosted(
            "record N {int n;} agent A {model=\"m\";response=N;} workflow Main(){A.run(\"p\");}",
            Host {
                provider: Some(fake),
                ..Host::default()
            }
        )
        .is_err());
    }
}
#[test]
fn tool_round_trip_preserves_assistant_call_and_result() {
    let fake = provider(vec![
        json!({"role":"assistant","content":null,"tool_calls":[{"id":"c1","type":"function","function":{"name":"Count","arguments":"{\"text\":\"hello\"}"}}]}),
        json!({"content":"five"}),
    ]);
    let source = r#"tool int Count(string text){handler="text.length";} policy Allowed {allowTools=[Count];maxToolCalls=1;} agent A {model="m";tools=[Count];policy=Allowed;maxTurns=2;} workflow string Main(){return A.run("count").text;}"#;
    assert!(
        matches!(hosted(source,Host {provider:Some(fake.clone()),..Host::default()}).unwrap().value,Value::String(s) if s=="five")
    );
    let requests = fake.requests.lock().unwrap();
    assert_eq!(requests[1]["messages"][2]["tool_calls"][0]["id"], "c1");
    assert_eq!(requests[1]["messages"][3]["tool_call_id"], "c1");
    assert_eq!(requests[1]["messages"][3]["content"], "5");
}
#[test]
fn policies_reject_capabilities_before_provider_or_handler() {
    let fake = provider(vec![]);
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let mut host = Host {
        provider: Some(fake.clone()),
        ..Host::default()
    };
    host.tools
        .register("test.write", Capability::FileWrite, move |_, _, _| {
            seen.fetch_add(1, Ordering::SeqCst);
            Ok(Value::String("done".into()))
        })
        .unwrap();
    let error=hosted(r#"tool string Write(string text){handler="test.write";} policy Deny {allowTools=[Write];maxToolCalls=1;allowFileWrite=false;} agent A {model="m";tools=[Write];policy=Deny;} workflow Main(){A.run("go");}"#,host).err().unwrap();
    assert!(error.message.contains("policy denies"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(fake.requests.lock().unwrap().is_empty());
}
#[test]
fn hallucinated_tools_are_never_dispatched() {
    let fake = provider(vec![
        json!({"content":null,"tool_calls":[{"id":"x","function":{"name":"Surprise","arguments":"{}"}}]}),
    ]);
    let e = hosted(
        "agent A {model=\"m\";} workflow Main(){A.run(\"go\");}",
        Host {
            provider: Some(fake),
            ..Host::default()
        },
    )
    .err()
    .unwrap();
    assert!(e.message.contains("budget") || e.message.contains("not offered"));
}
#[test]
fn transport_retries_are_bounded() {
    let fake = Arc::new(Fake {
        answers: Mutex::new(vec![Err("offline".into()), Ok(json!({"content":"ok"}))].into()),
        ..Fake::default()
    });
    hosted(
        "agent A {model=\"m\";retries=1;} workflow Main(){A.run(\"p\");}",
        Host {
            provider: Some(fake.clone()),
            ..Host::default()
        },
    )
    .unwrap();
    assert_eq!(fake.requests.lock().unwrap().len(), 2);
}
#[test]
fn audio_inference_library_executes_capture_and_both_agents() {
    let fake = provider(vec![
        json!({"content":"Ship Friday."}),
        json!({"content":"{\"topic\":\"launch\",\"summary\":\"Ship Friday.\",\"keyPoints\":[\"Friday\"],\"actionItems\":[],\"uncertainties\":[]}"}),
    ]);
    let captured = Arc::new(AtomicUsize::new(0));
    let seen = captured.clone();
    let host = Host {
        provider: Some(fake.clone()),
        recorder: Arc::new(move |seconds| {
            assert_eq!(seconds, Some(30));
            seen.fetch_add(1, Ordering::SeqCst);
            AudioInput::from_wav_bytes("fake.wav", b"RIFF-test".to_vec()).map_err(|e| e.to_string())
        }),
        ..Host::default()
    };
    let p = CompiledProgram::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../Camel/examples/audio_note_summarizer.camel"
    ))
    .unwrap();
    let result = Engine::new(p, host)
        .run("Main", vec![Value::String("launch".into())])
        .unwrap();
    assert_eq!(captured.load(Ordering::SeqCst), 1);
    let requests = fake.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0]["messages"][1]["content"][1]["type"],
        "input_audio"
    );
    assert!(requests[1]["messages"][1]["content"]
        .as_str()
        .unwrap()
        .contains("launch"));
    assert!(matches!(result.value, Value::Object(_)));
}
#[test]
fn imports_resolve_functions_and_detect_cycles() {
    let dir = tempfile::tempdir().unwrap();
    let math = dir.path().join("math.camel");
    let entry = dir.path().join("main.camel");
    std::fs::write(&math, "int Twice(int n){return n*2;}").unwrap();
    std::fs::write(
        &entry,
        "using \"math.camel\";workflow int Main(){return Twice(3);}",
    )
    .unwrap();
    let p = CompiledProgram::load(&entry).unwrap();
    assert!(matches!(
        Engine::new(p, Host::default())
            .run("Main", vec![])
            .unwrap()
            .value,
        Value::Int(6)
    ));
    std::fs::write(math, "using \"main.camel\";").unwrap();
    assert!(CompiledProgram::load(entry)
        .err()
        .unwrap()
        .message
        .contains("cyclic import"));
}

#[test]
fn imports_reject_conflicting_language_versions() {
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("library.camel");
    let entry = dir.path().join("main.camel");
    std::fs::write(&library, "version \"0.3\"; int Twice(int n){return n*2;}").unwrap();
    std::fs::write(
        &entry,
        "version \"0.2\"; using \"library.camel\"; workflow int Main(){return Twice(3);}",
    )
    .unwrap();
    let error = CompiledProgram::load(entry).err().unwrap();
    assert!(error.message.contains("language version"));
}

#[test]
fn file_tools_write_atomically_and_reject_escape() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("main.camel");
    std::fs::write(&entry,r#"tool string Write(string path,string text){handler="files.write";} tool string Read(string path){handler="files.read";} workflow string Main(){Write("note.txt","first");Write("note.txt","second");try{Write("../escape.txt","bad");}catch(e){}return Read("note.txt");}"#).unwrap();
    let p = CompiledProgram::load(entry).unwrap();
    let result = Engine::new(p, Host::default()).run("Main", vec![]).unwrap();
    assert!(matches!(result.value,Value::String(s) if s=="second"));
    assert!(!dir.path().parent().unwrap().join("escape.txt").exists());
}
