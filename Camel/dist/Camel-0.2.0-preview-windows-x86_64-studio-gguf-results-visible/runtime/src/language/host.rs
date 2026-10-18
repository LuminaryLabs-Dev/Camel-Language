use super::{syntax::*, value::Value};
use crate::{record_microphone, AudioInput, LmStudioClient};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
pub trait ModelProvider: Send + Sync {
    fn complete(
        &self,
        request: serde_json::Value,
    ) -> std::result::Result<serde_json::Value, String>;
}
impl ModelProvider for LmStudioClient {
    fn complete(
        &self,
        request: serde_json::Value,
    ) -> std::result::Result<serde_json::Value, String> {
        self.complete_message(&request).map_err(|e| e.to_string())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    Pure,
    FileRead,
    FileWrite,
    Network,
    Shell,
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
    pub tools: ToolRegistry,
    pub recorder: Recorder,
    pub save_recording: Option<PathBuf>,
    pub recording_seconds: Option<Option<u64>>,
    pub observer: Option<Observer>,
}
impl Default for Host {
    fn default() -> Self {
        Self {
            provider: None,
            tools: ToolRegistry::standard(),
            recorder: Arc::new(|s| record_microphone(s).map_err(|e| e.to_string())),
            save_recording: None,
            recording_seconds: None,
            observer: None,
        }
    }
}
