use camel_studio::{
    runner::{Runner, Update},
    worker::Message,
};
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[test]
fn real_worker_executes_offline_and_reports_source_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.camel");
    std::fs::write(
        &path,
        "workflow int Main(int count = 6) { log(\"working\"); return count * 2; }",
    )
    .unwrap();
    let mut runner = Runner::start(
        Path::new(env!("CARGO_BIN_EXE_camel-studio")),
        &path,
        "Main",
        &["7".into()],
        "http://localhost:1234",
        &Default::default(),
        &[],
    )
    .unwrap();
    let (messages, success) = drain(&mut runner);
    assert!(success);
    assert!(messages
        .iter()
        .any(|u| matches!(u,Update::Message(Message::Result{value}) if value==14)));
    assert!(messages
        .iter()
        .any(|u| matches!(u, Update::Message(Message::Event { .. }))));
    std::fs::write(
        &path,
        "workflow void Main() { throw \"expected failure\"; }",
    )
    .unwrap();
    let mut runner = Runner::start(
        Path::new(env!("CARGO_BIN_EXE_camel-studio")),
        &path,
        "Main",
        &[],
        "http://localhost:1234",
        &Default::default(),
        &[],
    )
    .unwrap();
    let (messages, success) = drain(&mut runner);
    assert!(!success);
    assert!(messages.iter().any(|u|matches!(u,Update::Message(Message::Diagnostic{error}) if error.message.contains("expected failure")&&error.site.line==1)));
}

#[test]
fn worker_rejects_model_calls_without_this_runs_approval() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("approval.camel");
    std::fs::write(
        &path,
        "agent A{model=\"local\";} workflow Main(){Agent a=A; await a.run(\"prompt\");}",
    )
    .unwrap();
    let mut runner = Runner::start(
        Path::new(env!("CARGO_BIN_EXE_camel-studio")),
        &path,
        "Main",
        &[],
        "http://127.0.0.1:1",
        &Default::default(),
        &[],
    )
    .unwrap();
    let (messages, success) = drain(&mut runner);
    assert!(!success);
    assert!(messages.iter().any(|update| matches!(
        update,
        Update::Message(Message::Diagnostic { error })
            if error.message.contains("approval required for capability 'model'")
    )));
}
#[test]
fn stop_is_repeat_safe_and_reaps_only_the_owned_worker() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.camel");
    std::fs::write(
        &path,
        "workflow void Main() { while (true) { log(\"busy\"); } }",
    )
    .unwrap();
    let mut runner = Runner::start(
        Path::new(env!("CARGO_BIN_EXE_camel-studio")),
        &path,
        "Main",
        &[],
        "http://localhost:1234",
        &Default::default(),
        &[],
    )
    .unwrap();
    runner.stop().unwrap();
    runner.stop().unwrap();
    assert!(runner.stopped);
    assert!(!drain(&mut runner).1);
}

#[test]
fn parallel_agent_workflow_round_trips_through_worker_and_http_adapter() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let start = Instant::now();
        let mut requests = Vec::new();
        while requests.len() < 2 {
            let (mut socket, _) = match listener.accept() {
                Ok(s) => s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        start.elapsed() < Duration::from_secs(10),
                        "missing agent request"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut chunk = [0u8; 4096];
            let body = loop {
                let n = socket.read(&mut chunk).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                assert!(bytes.len() < 65536);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..end]);
                    assert!(header.starts_with("POST /v1/chat/completions HTTP/1.1"));
                    let length = header
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|n| n.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    if bytes.len() >= end + 4 + length {
                        break serde_json::from_slice::<serde_json::Value>(
                            &bytes[end + 4..end + 4 + length],
                        )
                        .unwrap();
                    }
                }
            };
            requests.push(body);
            let response=serde_json::json!({"choices":[{"message":{"role":"assistant","content":"{\"observations\":[\"Security approval pending\"],\"uncertainties\":[\"Release date unconfirmed\"]}"}}],"usage":{"total_tokens":8}}).to_string();
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
        }
        requests
    });
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("Camel/examples/orchestration.camel");
    let mut runner = Runner::start(
        Path::new(env!("CARGO_BIN_EXE_camel-studio")),
        &source,
        "Main",
        &[],
        &endpoint,
        &Default::default(),
        &["model".into()],
    )
    .unwrap();
    let (messages, success) = drain(&mut runner);
    assert!(success);
    let agents=messages.iter().filter(|u|matches!(u,Update::Message(Message::Event{data:camel_audio_runtime::language::ExecutionEvent::Started{kind,..}}) if kind=="agent")).count();
    assert_eq!(agents, 2);
    assert!(messages.iter().any(|u|matches!(u,Update::Message(Message::Result{value}) if value["decisions"]["observations"][0]=="Security approval pending"&&value["risks"]["uncertainties"][0]=="Release date unconfirmed")));
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|r| r["messages"][1]["content"]
        .as_str()
        .unwrap()
        .contains("launch readiness")));
}
fn drain(runner: &mut Runner) -> (Vec<Update>, bool) {
    let start = Instant::now();
    let mut messages = Vec::new();
    loop {
        let (batch, done) = runner.poll().unwrap();
        messages.extend(batch);
        if let Some(success) = done {
            return (messages, success);
        }
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "worker did not finish"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
