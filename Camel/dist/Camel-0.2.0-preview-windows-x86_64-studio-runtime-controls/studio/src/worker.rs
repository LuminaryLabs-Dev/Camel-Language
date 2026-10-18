//! The IDE's private, versioned JSON-lines worker protocol. No shell or cargo subprocess.
use camel_audio_runtime::{
    language::{CancellationToken, CompiledProgram, Engine, Error, ExecutionEvent, Host, Site},
    LmStudioClient,
};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "message", rename_all = "snake_case")]
pub enum Message {
    Event { data: ExecutionEvent },
    Result { value: serde_json::Value },
    Diagnostic { error: Error },
}
pub fn execute(
    path: &Path,
    entry: &str,
    raw: &[String],
    send: Arc<dyn Fn(Message) + Send + Sync>,
) -> bool {
    execute_inner(path, entry, raw, send, None)
}
fn execute_inner(
    path: &Path,
    entry: &str,
    raw: &[String],
    send: Arc<dyn Fn(Message) + Send + Sync>,
    cancel_file: Option<PathBuf>,
) -> bool {
    let site = Site {
        file: path.display().to_string(),
        line: 1,
        column: 1,
        offset: 0,
        end_line: 1,
        end_column: 1,
        end_offset: 0,
    };
    let result = (|| {
        let program = CompiledProgram::load(path)?;
        let args = program.parse_arguments(entry, raw)?;
        let client =
            LmStudioClient::from_environment().map_err(|e| Error::at(&site, e.to_string()))?;
        let sink = send.clone();
        let cancellation = CancellationToken::default();
        if let Some(cancel_file) = cancel_file {
            let signal = cancellation.clone();
            std::thread::spawn(move || loop {
                match std::fs::read(&cancel_file) {
                    Ok(value) if value == b"1" => {
                        signal.cancel();
                        break;
                    }
                    Err(_) => break,
                    _ => std::thread::sleep(std::time::Duration::from_millis(25)),
                }
            });
        }
        let host = Host {
            provider: Some(Arc::new(client)),
            observer: Some(Arc::new(move |data| sink(Message::Event { data }))),
            cancellation,
            ..Host::default()
        };
        let result = Engine::new(program, host).run(entry, args)?;
        result.value.to_json(&site)
    })();
    match result {
        Ok(value) => {
            send(Message::Result { value });
            true
        }
        Err(error) => {
            send(Message::Diagnostic { error });
            false
        }
    }
}
pub fn main(args: &[String]) -> i32 {
    if args.len() != 4 {
        eprintln!("Worker expects source, workflow, JSON arguments, and cancellation signal path");
        return 2;
    }
    let raw: Vec<String> = match serde_json::from_str(&args[2]) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Invalid worker arguments: {e}");
            return 2;
        }
    };
    let output = Arc::new(Mutex::new(std::io::stdout()));
    let send = Arc::new(move |message| {
        let mut output = output.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(line) = serde_json::to_vec(&message) {
            let _ = output
                .write_all(&line)
                .and_then(|_| output.write_all(b"\n"))
                .and_then(|_| output.flush());
        }
    });
    if execute_inner(
        Path::new(&args[0]),
        &args[1],
        &raw,
        send,
        Some(PathBuf::from(&args[3])),
    ) {
        0
    } else {
        1
    }
}
