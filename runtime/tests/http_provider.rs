use camel_audio_runtime::{
    language::{CompiledProgram, Engine, Host, Value},
    LmStudioClient,
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    time::Duration,
};

#[test]
fn interpreted_agent_round_trips_through_real_http_adapter() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 4096];
        let (header_len, body_len) = loop {
            let n = socket.read(&mut chunk).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            assert!(bytes.len() < 65536);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&bytes[..end]);
                let length = header
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|n| n.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break (end + 4, length);
                }
            }
        };
        let header = String::from_utf8_lossy(&bytes[..header_len]);
        assert!(header.starts_with("POST /v1/chat/completions HTTP/1.1"));
        assert!(header
            .to_ascii_lowercase()
            .contains("authorization: bearer test-token"));
        let request: serde_json::Value =
            serde_json::from_slice(&bytes[header_len..header_len + body_len]).unwrap();
        let response = r#"{"choices":[{"message":{"role":"assistant","content":"{\"answer\":7}"}}],"usage":{"total_tokens":12}}"#;
        write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
        request
    });
    let client =
        LmStudioClient::with_endpoint(&endpoint, Some("test-token".into()), Duration::from_secs(5))
            .unwrap();
    let p=CompiledProgram::from_source("http.camel",r#"record Answer {int answer;} agent Worker {model="test-model";response=Answer;system="Test instruction";} workflow int Main(){return Worker.run("question").data.answer;}"#).unwrap();
    let r = Engine::new(
        p,
        Host {
            provider: Some(Arc::new(client)),
            ..Host::default()
        },
    )
    .run("Main", vec![])
    .unwrap();
    assert!(matches!(r.value, Value::Int(7)));
    let request = server.join().unwrap();
    assert_eq!(request["model"], "test-model");
    assert_eq!(request["messages"][1]["content"], "question");
    assert_eq!(request["max_tokens"], 32_768);
}
