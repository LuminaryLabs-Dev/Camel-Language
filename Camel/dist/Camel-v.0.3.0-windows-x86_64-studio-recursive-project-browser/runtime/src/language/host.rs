use super::{syntax::*, value::Value};
use crate::{record_microphone, AudioInput, LmStudioClient};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Clone)]
pub struct ReplayCassette(Arc<std::sync::Mutex<CassetteState>>);
struct CassetteState {
    path: PathBuf,
    replay: bool,
    continuing: bool,
    cursor: usize,
    events: Vec<CassetteEvent>,
    source_fingerprint: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct CassetteEvent {
    kind: String,
    input: Json,
    output: std::result::Result<Json, String>,
}
#[derive(Serialize, Deserialize)]
struct CassetteFile {
    version: u32,
    #[serde(default)]
    source_fingerprint: Option<String>,
    events: Vec<CassetteEvent>,
}
impl ReplayCassette {
    pub fn record_to(path: impl Into<PathBuf>) -> std::result::Result<Self, String> {
        let path = path.into();
        if path.exists() {
            return Err(format!("cassette already exists: {}", path.display()));
        }
        let cassette = Self(Arc::new(std::sync::Mutex::new(CassetteState {
            path,
            replay: false,
            continuing: false,
            cursor: 0,
            events: Vec::new(),
            source_fingerprint: None,
        })));
        cassette.persist()?;
        Ok(cassette)
    }
    pub fn replay_from(path: impl Into<PathBuf>) -> std::result::Result<Self, String> {
        let path = path.into();
        let file: CassetteFile = serde_json::from_slice(
            &std::fs::read(&path).map_err(|error| format!("cannot read cassette: {error}"))?,
        )
        .map_err(|error| format!("invalid cassette: {error}"))?;
        if file.version != 1 {
            return Err(format!("unsupported cassette version {}", file.version));
        }
        Ok(Self(Arc::new(std::sync::Mutex::new(CassetteState {
            path,
            replay: true,
            continuing: false,
            cursor: 0,
            events: file.events,
            source_fingerprint: file.source_fingerprint,
        }))))
    }
    pub fn continue_recording(path: impl Into<PathBuf>) -> std::result::Result<Self, String> {
        let path = path.into();
        let file: CassetteFile = serde_json::from_slice(
            &std::fs::read(&path).map_err(|error| format!("cannot read cassette: {error}"))?,
        )
        .map_err(|error| format!("invalid cassette: {error}"))?;
        if file.version != 1 {
            return Err(format!("unsupported cassette version {}", file.version));
        }
        let cursor = file.events.len();
        Ok(Self(Arc::new(std::sync::Mutex::new(CassetteState {
            path,
            replay: false,
            continuing: true,
            cursor,
            events: file.events,
            source_fingerprint: file.source_fingerprint,
        }))))
    }
    pub(crate) fn is_replay(&self) -> bool {
        self.0.lock().unwrap().replay
    }
    pub(crate) fn exchange(
        &self,
        kind: &str,
        input: Json,
        action: impl FnOnce() -> std::result::Result<Json, String>,
    ) -> std::result::Result<Json, String> {
        let mut state = self.0.lock().unwrap();
        if state.replay || (state.continuing && state.cursor < state.events.len()) {
            let event = state
                .events
                .get(state.cursor)
                .cloned()
                .ok_or_else(|| format!("cassette exhausted at {kind}"))?;
            if event.kind != kind || event.input != input {
                return Err(format!(
                    "cassette mismatch at event {}: expected {}, received {}",
                    state.cursor + 1,
                    event.kind,
                    kind
                ));
            }
            state.cursor += 1;
            return event.output;
        }
        let output = action();
        state.events.push(CassetteEvent {
            kind: kind.into(),
            input,
            output: output.clone(),
        });
        state.cursor = state.events.len();
        let file = CassetteFile {
            version: 1,
            source_fingerprint: state.source_fingerprint.clone(),
            events: state.events.clone(),
        };
        persist_cassette(&state.path, &file)?;
        output
    }
    pub(crate) fn ensure_consumed(&self) -> std::result::Result<(), String> {
        let state = self.0.lock().unwrap();
        if (state.replay || state.continuing) && state.cursor != state.events.len() {
            Err(format!(
                "cassette has {} unconsumed event(s)",
                state.events.len() - state.cursor
            ))
        } else {
            Ok(())
        }
    }
    pub(crate) fn cursor(&self) -> usize {
        self.0.lock().unwrap().cursor
    }
    pub(crate) fn bind_source(&self, fingerprint: &str) -> std::result::Result<(), String> {
        let mut state = self.0.lock().unwrap();
        if state.replay {
            return if state.source_fingerprint.as_deref() == Some(fingerprint) {
                Ok(())
            } else {
                Err("cassette does not match this source fingerprint".into())
            };
        }
        if state
            .source_fingerprint
            .as_deref()
            .is_some_and(|saved| saved != fingerprint)
        {
            return Err("cassette is already bound to different source".into());
        }
        state.source_fingerprint = Some(fingerprint.into());
        persist_cassette(
            &state.path,
            &CassetteFile {
                version: 1,
                source_fingerprint: state.source_fingerprint.clone(),
                events: state.events.clone(),
            },
        )
    }
    pub(crate) fn seek(&self, cursor: usize) -> std::result::Result<(), String> {
        let mut state = self.0.lock().unwrap();
        if cursor > state.events.len() {
            return Err("checkpoint cassette position exceeds cassette length".into());
        }
        state.cursor = cursor;
        if !state.replay && !state.continuing {
            state.events.truncate(cursor);
            let file = CassetteFile {
                version: 1,
                source_fingerprint: state.source_fingerprint.clone(),
                events: state.events.clone(),
            };
            persist_cassette(&state.path, &file)?;
        }
        Ok(())
    }
    pub fn fingerprint(&self) -> String {
        let state = self.0.lock().unwrap();
        stable_fingerprint(
            &serde_json::to_vec(&(&state.source_fingerprint, &state.events)).unwrap_or_default(),
        )
    }
    fn persist(&self) -> std::result::Result<(), String> {
        let state = self.0.lock().unwrap();
        persist_cassette(
            &state.path,
            &CassetteFile {
                version: 1,
                source_fingerprint: state.source_fingerprint.clone(),
                events: state.events.clone(),
            },
        )
    }
}
fn persist_cassette(path: &Path, file: &CassetteFile) -> std::result::Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut pending = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("cannot create cassette: {error}"))?;
    serde_json::to_writer_pretty(&mut pending, file)
        .map_err(|error| format!("cannot encode cassette: {error}"))?;
    pending
        .as_file()
        .sync_all()
        .map_err(|error| format!("cannot sync cassette: {error}"))?;
    pending
        .persist(path)
        .map_err(|error| format!("cannot save cassette: {}", error.error))?;
    Ok(())
}
fn stable_fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}
#[derive(Clone, Debug, Default)]
pub struct ProviderCapabilities {
    pub tool_calls: bool,
    pub structured_output: bool,
    pub token_usage: bool,
}
pub trait ModelProvider: Send + Sync {
    /// Advertise features before a run sends a request that depends on them.
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            tool_calls: false,
            structured_output: false,
            token_usage: false,
        }
    }
    fn complete(
        &self,
        request: serde_json::Value,
    ) -> std::result::Result<serde_json::Value, String>;
}
impl ModelProvider for LmStudioClient {
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            tool_calls: true,
            structured_output: true,
            token_usage: true,
        }
    }
    fn complete(
        &self,
        request: serde_json::Value,
    ) -> std::result::Result<serde_json::Value, String> {
        self.complete_message(&request).map_err(|e| e.to_string())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    Pure,
    FileRead,
    FileWrite,
    Network,
    Shell,
    Model,
    Microphone,
}
#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
type Handler = Arc<dyn Fn(&[Value], &Path, &Site) -> Result<Value> + Send + Sync>;
#[derive(Clone, Default)]
pub struct ToolRegistry {
    handlers: BTreeMap<String, (Capability, Handler)>,
}
impl ToolRegistry {
    pub fn register(
        &mut self,
        name: impl Into<String>,
        capability: Capability,
        handler: impl Fn(&[Value], &Path, &Site) -> Result<Value> + Send + Sync + 'static,
    ) -> std::result::Result<(), String> {
        let name = name.into();
        if self.handlers.contains_key(&name) {
            return Err(format!("handler '{name}' already registered"));
        }
        self.handlers.insert(name, (capability, Arc::new(handler)));
        Ok(())
    }
    pub fn standard() -> Self {
        let mut r = Self::default();
        r.register("text.length", Capability::Pure, |a, _, s| {
            count(a, 1, s)?;
            Ok(Value::Int(a[0].text(s)?.chars().count() as i64))
        })
        .unwrap();
        r.register("json.stringify", Capability::Pure, |a, _, s| {
            count(a, 1, s)?;
            Ok(Value::String(a[0].to_json(s)?.to_string()))
        })
        .unwrap();
        r.register("files.read", Capability::FileRead, |a, root, s| {
            count(a, 1, s)?;
            let path = read_path(root, &a[0].text(s)?, s)?;
            std::fs::read_to_string(path)
                .map(Value::String)
                .map_err(|e| Error::at(s, e.to_string()))
        })
        .unwrap();
        r.register("files.write", Capability::FileWrite, |a, root, s| {
            count(a, 2, s)?;
            let requested = a[0].text(s)?;
            let content = a[1].text(s)?;
            let relative = Path::new(&requested);
            if relative.is_absolute() {
                return Err(Error::at(s, "file path must be relative"));
            }
            let path = root.join(relative);
            let parent = path
                .parent()
                .ok_or_else(|| Error::at(s, "missing parent"))?
                .canonicalize()
                .map_err(|e| Error::at(s, e.to_string()))?;
            let canonical_root = root
                .canonicalize()
                .map_err(|e| Error::at(s, e.to_string()))?;
            if !parent.starts_with(&canonical_root) {
                return Err(Error::at(s, "file path escapes program directory"));
            }
            let name = path
                .file_name()
                .ok_or_else(|| Error::at(s, "missing filename"))?;
            let target = parent.join(name);
            if target.exists() {
                let resolved = target
                    .canonicalize()
                    .map_err(|e| Error::at(s, e.to_string()))?;
                if !resolved.starts_with(&canonical_root) {
                    return Err(Error::at(s, "file symlink escapes program directory"));
                }
            }
            use std::io::Write;
            let mut pending = tempfile::NamedTempFile::new_in(&parent)
                .map_err(|e| Error::at(s, e.to_string()))?;
            pending
                .write_all(content.as_bytes())
                .and_then(|_| pending.flush())
                .and_then(|_| pending.as_file().sync_all())
                .map_err(|e| Error::at(s, e.to_string()))?;
            pending
                .persist(&target)
                .map_err(|e| Error::at(s, e.to_string()))?;
            Ok(Value::String(requested))
        })
        .unwrap();
        r.register("http.get", Capability::Network, |a, _, s| {
            count(a, 1, s)?;
            let url = a[0].text(s)?;
            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| Error::at(s, e.to_string()))?;
            client
                .get(url)
                .send()
                .and_then(|r| r.error_for_status())
                .and_then(|r| r.text())
                .map(Value::String)
                .map_err(|e| Error::at(s, e.to_string()))
        })
        .unwrap();
        r
    }
    pub(crate) fn capability(&self, name: &str, s: &Site) -> Result<Capability> {
        self.handlers
            .get(name)
            .map(|(c, _)| *c)
            .ok_or_else(|| Error::at(s, format!("unregistered tool handler '{name}'")))
    }
    pub(crate) fn invoke(&self, name: &str, a: &[Value], root: &Path, s: &Site) -> Result<Value> {
        let (_, handler) = self
            .handlers
            .get(name)
            .ok_or_else(|| Error::at(s, format!("unregistered tool handler '{name}'")))?;
        handler(a, root, s)
    }
}
fn read_path(root: &Path, requested: &str, s: &Site) -> Result<PathBuf> {
    if Path::new(requested).is_absolute() {
        return Err(Error::at(s, "file path must be relative"));
    }
    let path = root
        .join(requested)
        .canonicalize()
        .map_err(|e| Error::at(s, e.to_string()))?;
    let root = root
        .canonicalize()
        .map_err(|e| Error::at(s, e.to_string()))?;
    if !path.starts_with(root) {
        return Err(Error::at(s, "file path escapes program directory"));
    }
    Ok(path)
}
pub(crate) fn count(a: &[Value], expected: usize, s: &Site) -> Result<()> {
    if a.len() == expected {
        Ok(())
    } else {
        Err(Error::at(
            s,
            format!("expected {expected} arguments, got {}", a.len()),
        ))
    }
}
pub type Recorder =
    Arc<dyn Fn(Option<u64>) -> std::result::Result<AudioInput, String> + Send + Sync>;
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ExecutionEvent {
    Started {
        id: u64,
        kind: String,
        name: String,
        detail: String,
        site: Site,
    },
    Finished {
        id: u64,
        success: bool,
        elapsed_ms: u64,
    },
    Budget {
        tokens_used: usize,
        tokens_limit: usize,
        tool_calls_used: usize,
        tool_calls_limit: usize,
        elapsed_ms: u64,
        deadline_ms: u64,
    },
    Log {
        text: String,
    },
}
pub type Observer = Arc<dyn Fn(ExecutionEvent) + Send + Sync>;
/// Optional telemetry must not change language execution, including on observer panic.
pub(crate) fn emit(host: &Host, event: ExecutionEvent) {
    if let Some(observer) = &host.observer {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| observer(event)));
    }
}
pub(crate) struct Operation {
    host: Host,
    id: u64,
    started: std::time::Instant,
    pub success: bool,
}
impl Operation {
    pub fn begin(host: &Host, kind: &str, name: &str, detail: &str, site: &Site) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        emit(
            host,
            ExecutionEvent::Started {
                id,
                kind: kind.into(),
                name: name.into(),
                detail: detail.into(),
                site: site.clone(),
            },
        );
        Self {
            host: host.clone(),
            id,
            started: std::time::Instant::now(),
            success: false,
        }
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        emit(
            &self.host,
            ExecutionEvent::Finished {
                id: self.id,
                success: self.success,
                elapsed_ms: self.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            },
        );
    }
}
#[derive(Clone)]
pub struct Host {
    pub provider: Option<Arc<dyn ModelProvider>>,
    /// Per-run model overrides keyed by Camel agent declaration name.
    pub agent_models: BTreeMap<String, String>,
    /// `Some` enables per-run capability approval; `None` preserves trusted embedding behavior.
    pub approved_capabilities: Option<std::collections::BTreeSet<Capability>>,
    pub tools: ToolRegistry,
    pub recorder: Recorder,
    pub save_recording: Option<PathBuf>,
    pub recording_seconds: Option<Option<u64>>,
    pub observer: Option<Observer>,
    pub cancellation: CancellationToken,
    pub cassette: Option<ReplayCassette>,
}
impl Default for Host {
    fn default() -> Self {
        Self {
            provider: None,
            agent_models: BTreeMap::new(),
            approved_capabilities: None,
            tools: ToolRegistry::standard(),
            recorder: Arc::new(|s| record_microphone(s).map_err(|e| e.to_string())),
            save_recording: None,
            recording_seconds: None,
            observer: None,
            cancellation: CancellationToken::default(),
            cassette: None,
        }
    }
}

impl Host {
    pub(crate) fn require_approval(&self, capability: Capability, site: &Site) -> Result<()> {
        if self
            .approved_capabilities
            .as_ref()
            .is_some_and(|approved| approved.contains(&capability))
            || self.approved_capabilities.is_none()
        {
            Ok(())
        } else {
            Err(Error::at(
                site,
                format!(
                    "run approval required for capability '{}'",
                    capability.name()
                ),
            ))
        }
    }
}

impl Capability {
    pub fn name(self) -> &'static str {
        match self {
            Self::Pure => "pure",
            Self::FileRead => "file-read",
            Self::FileWrite => "file-write",
            Self::Network => "network",
            Self::Shell => "shell",
            Self::Model => "model",
            Self::Microphone => "microphone",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "file-read" => Self::FileRead,
            "file-write" => Self::FileWrite,
            "network" => Self::Network,
            "shell" => Self::Shell,
            "model" => Self::Model,
            "microphone" => Self::Microphone,
            _ => return None,
        })
    }
}
