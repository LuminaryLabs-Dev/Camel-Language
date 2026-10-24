//! The IDE's private, versioned JSON-lines worker protocol. No shell or cargo subprocess.
use camel_audio_runtime::{
    language::{
        CancellationToken, Capability, CompiledProgram, Engine, Error, ExecutionEvent, Host,
        Limits, ModelProvider, ReplayCassette, Site,
    },
    LmStudioClient,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
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
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RunArtifacts {
    pub checkpoint: Option<PathBuf>,
    pub resume: Option<PathBuf>,
    pub record_cassette: Option<PathBuf>,
    pub replay_cassette: Option<PathBuf>,
    pub evaluation_report: Option<PathBuf>,
}
struct WorkerOptions {
    cancel_file: Option<PathBuf>,
    agent_models: BTreeMap<String, String>,
    approved_capabilities: Option<BTreeSet<Capability>>,
    artifacts: RunArtifacts,
}
pub fn execute(
    path: &Path,
    entry: &str,
    raw: &[String],
    send: Arc<dyn Fn(Message) + Send + Sync>,
) -> bool {
    execute_inner(
        path,
        entry,
        raw,
        send,
        WorkerOptions {
            cancel_file: None,
            agent_models: BTreeMap::new(),
            approved_capabilities: None,
            artifacts: RunArtifacts::default(),
        },
    )
}
fn execute_inner(
    path: &Path,
    entry: &str,
    raw: &[String],
    send: Arc<dyn Fn(Message) + Send + Sync>,
    options: WorkerOptions,
) -> bool {
    let WorkerOptions {
        cancel_file,
        agent_models,
        approved_capabilities,
        artifacts,
    } = options;
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
        if artifacts.record_cassette.is_some() && artifacts.replay_cassette.is_some() {
            return Err(Error::at(
                &site,
                "record and replay cassette modes are mutually exclusive",
            ));
        }
        if artifacts.evaluation_report.is_some() && artifacts.replay_cassette.is_none() {
            return Err(Error::at(&site, "evaluation report requires replay mode"));
        }
        let cassette = if let Some(path) = &artifacts.record_cassette {
            let cassette = if artifacts.resume.is_some() {
                ReplayCassette::continue_recording(path)
            } else {
                ReplayCassette::record_to(path)
            };
            Some(cassette.map_err(|e| Error::at(&site, e))?)
        } else if let Some(path) = &artifacts.replay_cassette {
            Some(ReplayCassette::replay_from(path).map_err(|e| Error::at(&site, e))?)
        } else {
            None
        };
        let client: Option<Arc<dyn ModelProvider>> = if artifacts.replay_cassette.is_some() {
            None
        } else {
            Some(Arc::new(
                LmStudioClient::from_environment().map_err(|e| Error::at(&site, e.to_string()))?,
            ))
        };
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
            provider: client,
            agent_models,
            approved_capabilities,
            observer: Some(Arc::new(move |data| sink(Message::Event { data }))),
            cancellation,
            cassette,
            ..Host::default()
        };
        let mut engine = Engine::new(program, host).with_limits(Limits::default());
        if let Some(path) = &artifacts.checkpoint {
            engine = engine.with_checkpoint_file(path);
        }
        if let Some(path) = &artifacts.resume {
            engine = engine.resume_from(path);
        }
        let result = engine.run(entry, args)?;
        if let Some(path) = &artifacts.evaluation_report {
            let report = serde_json::json!({
                "format": "camel-evaluation-v1",
                "sourceFingerprint": result.source_fingerprint,
                "cassetteFingerprint": result.cassette_fingerprint,
                "workflow": entry,
                "result": result.value.to_json(&site)?,
                "logs": result.logs,
                "usage": {"tokens": result.tokens_used, "toolCalls": result.tool_calls_used}
            });
            write_report_atomic(path, &report).map_err(|e| Error::at(&site, e))?;
        }
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
    if args.len() != 7 {
        eprintln!("Worker expects source, workflow, JSON arguments, cancellation signal path, agent model overrides, capability approvals, and run artifact options");
        return 2;
    }
    let raw: Vec<String> = match serde_json::from_str(&args[2]) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Invalid worker arguments: {e}");
            return 2;
        }
    };
    let agent_models: BTreeMap<String, String> = match serde_json::from_str(&args[4]) {
        Ok(models) => models,
        Err(e) => {
            eprintln!("Invalid agent model overrides: {e}");
            return 2;
        }
    };
    let approval_names: Vec<String> = match serde_json::from_str(&args[5]) {
        Ok(approvals) => approvals,
        Err(e) => {
            eprintln!("Invalid capability approvals: {e}");
            return 2;
        }
    };
    let artifacts: RunArtifacts = match serde_json::from_str(&args[6]) {
        Ok(artifacts) => artifacts,
        Err(error) => {
            eprintln!("Invalid run artifact options: {error}");
            return 2;
        }
    };
    let approved_capabilities: BTreeSet<Capability> = match approval_names
        .iter()
        .map(|name| Capability::parse(name).ok_or_else(|| format!("unknown capability '{name}'")))
        .collect()
    {
        Ok(approvals) => approvals,
        Err(e) => {
            eprintln!("Invalid capability approval: {e}");
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
        WorkerOptions {
            cancel_file: Some(PathBuf::from(&args[3])),
            agent_models,
            approved_capabilities: Some(approved_capabilities),
            artifacts,
        },
    ) {
        0
    } else {
        1
    }
}
fn write_report_atomic(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut pending = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut pending, value).map_err(|e| e.to_string())?;
    pending.as_file().sync_all().map_err(|e| e.to_string())?;
    pending.persist(path).map_err(|e| e.error.to_string())?;
    Ok(())
}
