use eframe::egui;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Seek, SeekFrom, Write},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

const HF_API: &str = "https://huggingface.co/api";
const DEFAULT_OLLAMA: &str = "http://localhost:11434";

#[derive(Clone, Debug, Deserialize)]
struct HubModel {
    id: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct HubFile {
    rfilename: String,
    #[serde(default)]
    size: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
struct HubRepository {
    id: String,
    #[serde(default)]
    sha: String,
    #[serde(default)]
    siblings: Vec<HubFile>,
}

#[derive(Clone, Debug, Deserialize)]
struct OllamaModel {
    name: String,
    #[serde(default)]
    size: u64,
}

enum Update {
    Search(Result<Vec<HubModel>, String>),
    Repository(Result<HubRepository, String>),
    Installed(Result<LocalModels, String>),
    Progress(String),
    Imported(Result<String, String>),
    ModelAction(Result<String, String>),
}

struct LocalModels {
    installed: Vec<OllamaModel>,
    running: BTreeSet<String>,
}

#[derive(Default)]
pub struct ModelManager {
    open: bool,
    ollama_endpoint: String,
    query: String,
    hub_token: String,
    models: Vec<HubModel>,
    repository: Option<HubRepository>,
    selected_model: Option<String>,
    selected_file: Option<String>,
    installed: Vec<OllamaModel>,
    running: BTreeSet<String>,
    selected_for_loading: BTreeSet<String>,
    context_sizes: BTreeMap<String, String>,
    busy: Option<String>,
    message: Option<String>,
    pending_refresh_message: Option<String>,
    updates: Option<Receiver<Update>>,
    use_endpoint_for_runs: bool,
}

impl ModelManager {
    pub fn available_model_names(&self) -> Vec<String> {
        self.installed
            .iter()
            .map(|model| model.name.clone())
            .collect()
    }

    pub fn open(&mut self) {
        self.open = true;
        if self.ollama_endpoint.is_empty() {
            self.ollama_endpoint = DEFAULT_OLLAMA.into();
        }
        if self.updates.is_none() {
            self.refresh_installed();
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        self.receive_updates();
        if self.updates.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Model Manager")
            .open(&mut open)
            .default_size([980.0, 650.0])
            .min_size([760.0, 480.0])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("model_manager_content")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.content(ui));
            });
        self.open = open;
    }

    fn content(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Ollama");
            ui.add(
                egui::TextEdit::singleline(&mut self.ollama_endpoint)
                    .desired_width(260.0)
                    .hint_text(DEFAULT_OLLAMA),
            );
            if ui
                .add_enabled(self.busy.is_none(), egui::Button::new("Refresh installed"))
                .clicked()
            {
                self.refresh_installed();
            }
            if ui.button("Use Ollama for workflow runs").clicked() {
                if endpoint(&self.ollama_endpoint).is_empty() {
                    self.message = Some("Enter an Ollama server address first.".into());
                } else {
                    self.use_endpoint_for_runs = true;
                    self.message =
                        Some("Ollama endpoint selected for the next workflow run.".into());
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Hugging Face token");
            ui.add(
                egui::TextEdit::singleline(&mut self.hub_token)
                    .desired_width(260.0)
                    .password(true)
                    .hint_text("Optional; kept in memory only"),
            );
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Find GGUF models");
            ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .desired_width(300.0)
                    .hint_text("Search Hugging Face"),
            );
            if ui
                .add_enabled(
                    self.busy.is_none() && !self.query.trim().is_empty(),
                    egui::Button::new("Search"),
                )
                .clicked()
            {
                self.search();
            }
            if let Some(busy) = &self.busy {
                ui.spinner();
                ui.label(busy);
            }
        });

        egui::CollapsingHeader::new("Hugging Face search results")
            .id_salt("hf_search_results")
            .default_open(true)
            .show(ui, |ui| self.search_results(ui));
        ui.add_space(8.0);
        self.selected_repository_files(ui);

        ui.separator();
        ui.strong("Models installed in Ollama");
        if self.installed.is_empty() {
            ui.weak("No local Ollama models listed. Check that Ollama is running.");
        }
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        self.busy.is_none() && !self.selected_for_loading.is_empty(),
                        egui::Button::new("Load selected models"),
                    )
                    .clicked()
                {
                    self.load_selected();
                }
                if !self.selected_for_loading.is_empty() {
                    ui.weak(format!("{} selected", self.selected_for_loading.len()));
                }
            });
            let installed = self.installed.clone();
            for model in installed {
                ui.horizontal(|ui| {
                    let mut selected = self.selected_for_loading.contains(&model.name);
                    if ui.checkbox(&mut selected, "").changed() {
                        if selected {
                            self.selected_for_loading.insert(model.name.clone());
                        } else {
                            self.selected_for_loading.remove(&model.name);
                        }
                    }
                    ui.monospace(&model.name);
                    ui.weak(format_bytes(model.size));
                    if self.running.contains(&model.name) {
                        ui.weak("Loaded");
                        if ui
                            .add_enabled(self.busy.is_none(), egui::Button::new("Unload"))
                            .clicked()
                        {
                            self.unload_model(model.name.clone());
                        }
                    } else {
                        ui.weak("Not loaded");
                        let size = self
                            .context_sizes
                            .entry(model.name.clone())
                            .or_insert_with(|| "4096".into());
                        ui.add(
                            egui::TextEdit::singleline(size)
                                .desired_width(70.0)
                                .hint_text("Context"),
                        );
                        if ui
                            .add_enabled(self.busy.is_none(), egui::Button::new("Load"))
                            .clicked()
                        {
                            let context = size.clone();
                            self.load_models(vec![(model.name.clone(), context)]);
                        }
                    }
                });
            }
        });
        ui.weak("Imports selected GGUF files only; model files are stored by Ollama locally.");
    }

    fn search_results(&mut self, ui: &mut egui::Ui) {
        let mut inspect = None;
        egui::ScrollArea::vertical()
            .id_salt("hf_repository_results")
            .max_height(220.0)
            .show(ui, |ui| {
                ui.strong("Hugging Face repositories");
                if self.models.is_empty() {
                    ui.weak("Search for repositories tagged with GGUF.");
                }
                for model in &self.models {
                    let selected = self.selected_model.as_deref() == Some(&model.id);
                    if ui
                        .selectable_label(selected, egui::RichText::new(&model.id).strong())
                        .clicked()
                    {
                        inspect = Some(model.id.clone());
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.weak(format!("{} downloads", model.downloads));
                        if model.tags.iter().any(|tag| tag == "text-generation") {
                            ui.weak("text generation");
                        }
                    });
                    ui.separator();
                }
            });
        if let Some(id) = inspect {
            self.load_repository(id);
        }
    }

    fn selected_repository_files(&mut self, ui: &mut egui::Ui) {
        ui.strong("Files in selected repository");
        let files = self
            .repository
            .as_ref()
            .map(|repo| {
                repo.siblings
                    .iter()
                    .filter(|file| file.rfilename.to_ascii_lowercase().ends_with(".gguf"))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if let Some(repo) = &self.repository {
            ui.label(&repo.id);
            if !repo.sha.is_empty() {
                ui.weak(format!("Pinned revision {}", short_revision(&repo.sha)));
            }
            if files.is_empty() {
                ui.weak("No GGUF files found in this repository.");
            }
        } else {
            ui.weak("Select a repository above to load its files.");
        }
        if let Some(message) = &self.message {
            ui.label(message);
        }
        if let Some(progress) = &self.busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(progress);
            });
        }
        ui.weak("Select one GGUF file:");
        for file in &files {
            let label = match file.size {
                Some(size) => format!("{} · {}", file.rfilename, format_bytes(size)),
                None => file.rfilename.clone(),
            };
            ui.selectable_value(&mut self.selected_file, Some(file.rfilename.clone()), label);
        }
        if files.len() > 1 && self.selected_file.is_none() {
            ui.weak("Select a GGUF file above to enable import.");
        }
        let can_import = self.busy.is_none()
            && self.repository.is_some()
            && self
                .selected_file
                .as_ref()
                .is_some_and(|selected| files.iter().any(|file| &file.rfilename == selected));
        if ui
            .add_enabled(
                can_import,
                egui::Button::new("Download and import into Ollama"),
            )
            .clicked()
        {
            self.import_selected();
        }
        if let Some(repo) = &self.repository {
            if let Some(file) = &self.selected_file {
                if files.iter().any(|candidate| &candidate.rfilename == file) {
                    let name = ollama_name(&repo.id, &repo.sha, file);
                    ui.add_space(8.0);
                    ui.label("Ollama model name");
                    ui.monospace(name);
                }
            }
        }
    }

    fn search(&mut self) {
        let query = self.query.trim().to_owned();
        let token = self.hub_token.trim().to_owned();
        self.start("Searching Hugging Face…", move |tx| {
            let result = search_hugging_face(&query, &token);
            let _ = tx.send(Update::Search(result));
        });
    }

    fn load_repository(&mut self, id: String) {
        self.selected_model = Some(id.clone());
        self.repository = None;
        self.selected_file = None;
        let token = self.hub_token.trim().to_owned();
        self.start("Loading repository files…", move |tx| {
            let result = repository_details(&id, &token);
            let _ = tx.send(Update::Repository(result));
        });
    }

    fn refresh_installed(&mut self) {
        let endpoint = endpoint(&self.ollama_endpoint);
        self.start("Checking Ollama…", move |tx| {
            let result = list_ollama_models(&endpoint);
            let _ = tx.send(Update::Installed(result));
        });
    }

    fn load_selected(&mut self) {
        let models = self
            .selected_for_loading
            .iter()
            .map(|name| {
                (
                    name.clone(),
                    self.context_sizes
                        .get(name)
                        .cloned()
                        .unwrap_or_else(|| "4096".into()),
                )
            })
            .collect();
        self.load_models(models);
    }

    fn load_models(&mut self, models: Vec<(String, String)>) {
        if models.is_empty() {
            return;
        }
        let endpoint = endpoint(&self.ollama_endpoint);
        self.start("Loading models into memory…", move |tx| {
            let total = models.len();
            for (index, (name, context)) in models.iter().enumerate() {
                let _ = tx.send(Update::Progress(format!(
                    "Loading model {} of {total}: {name}",
                    index + 1
                )));
                if let Err(error) = load_ollama_model(&endpoint, name, context) {
                    let _ = tx.send(Update::ModelAction(Err(error)));
                    return;
                }
            }
            let names = models
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let _ = tx.send(Update::ModelAction(Ok(format!(
                "Loaded {total} model(s): {names}. Ollama may unload models if memory is tight."
            ))));
        });
    }

    fn unload_model(&mut self, name: String) {
        let endpoint = endpoint(&self.ollama_endpoint);
        self.start("Unloading model…", move |tx| {
            let result = unload_ollama_model(&endpoint, &name).map(|_| format!("Unloaded {name}."));
            let _ = tx.send(Update::ModelAction(result));
        });
    }

    fn import_selected(&mut self) {
        let Some(repo) = self.repository.clone() else {
            return;
        };
        let Some(file) = self.selected_file.clone() else {
            return;
        };
        if !repo.siblings.iter().any(|item| item.rfilename == file) {
            return;
        }
        let ollama = endpoint(&self.ollama_endpoint);
        let token = self.hub_token.trim().to_owned();
        self.start("Preparing Hugging Face download…", move |tx| {
            let result = import_gguf(&repo, &file, &ollama, &token, &tx);
            let _ = tx.send(Update::Imported(result));
        });
    }

    fn start(&mut self, label: &str, operation: impl FnOnce(Sender<Update>) + Send + 'static) {
        if self.updates.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.updates = Some(rx);
        self.busy = Some(label.into());
        self.message = None;
        if let Err(error) = thread::Builder::new()
            .name("studio-model-manager".into())
            .spawn(move || operation(tx))
        {
            self.updates = None;
            self.busy = None;
            self.message = Some(format!("Could not start model operation: {error}"));
        }
    }

    fn receive_updates(&mut self) {
        if self.updates.is_none() {
            return;
        }
        let mut finished = false;
        let mut imported = false;
        let mut refresh = false;
        loop {
            let update = match self.updates.as_ref().unwrap().try_recv() {
                Ok(update) => update,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.message = Some("Model operation stopped unexpectedly.".into());
                    finished = true;
                    break;
                }
            };
            match update {
                Update::Progress(message) => self.busy = Some(message),
                Update::Search(Ok(models)) => {
                    self.models = models;
                    self.message = Some(format!("Found {} GGUF repositories.", self.models.len()));
                    finished = true;
                }
                Update::Search(Err(error)) => {
                    self.message = Some(format!("Hugging Face search failed: {error}"));
                    finished = true;
                }
                Update::Repository(Ok(repo)) => {
                    let only_gguf_file = repo
                        .siblings
                        .iter()
                        .filter(|file| file.rfilename.to_ascii_lowercase().ends_with(".gguf"))
                        .map(|file| file.rfilename.clone())
                        .collect::<Vec<_>>();
                    self.selected_file = if only_gguf_file.len() == 1 {
                        only_gguf_file.into_iter().next()
                    } else {
                        None
                    };
                    self.repository = Some(repo);
                    self.message = Some("Repository metadata loaded.".into());
                    finished = true;
                }
                Update::Repository(Err(error)) => {
                    self.message = Some(format!("Could not inspect repository: {error}"));
                    finished = true;
                }
                Update::Installed(Ok(models)) => {
                    self.installed = models.installed;
                    self.running = models.running;
                    self.selected_for_loading
                        .retain(|name| self.installed.iter().any(|model| &model.name == name));
                    self.message = Some(
                        self.pending_refresh_message
                            .take()
                            .unwrap_or_else(|| "Ollama model list refreshed.".into()),
                    );
                    finished = true;
                }
                Update::Installed(Err(error)) => {
                    self.message = Some(match self.pending_refresh_message.take() {
                        Some(action) => {
                            format!("{action} Current model status could not be refreshed: {error}")
                        }
                        None => format!("Could not reach Ollama: {error}"),
                    });
                    finished = true;
                }
                Update::Imported(Ok(name)) => {
                    self.message = Some(format!(
                        "Imported as {name}. Use that model name in a Camel agent."
                    ));
                    finished = true;
                    imported = true;
                }
                Update::Imported(Err(error)) => {
                    self.message = Some(format!("Model import failed: {error}"));
                    finished = true;
                }
                Update::ModelAction(Ok(message)) => {
                    self.message = Some(message);
                    finished = true;
                    refresh = true;
                }
                Update::ModelAction(Err(error)) => {
                    self.pending_refresh_message =
                        Some(format!("Ollama model operation failed: {error}"));
                    finished = true;
                    refresh = true;
                }
            }
            if finished {
                break;
            }
        }
        if finished {
            self.updates = None;
            self.busy = None;
        }
        if imported {
            self.refresh_after_import();
        } else if refresh {
            self.refresh_installed();
        }
    }

    fn refresh_after_import(&mut self) {
        let endpoint = endpoint(&self.ollama_endpoint);
        let (tx, rx) = mpsc::channel();
        self.updates = Some(rx);
        self.busy = Some("Refreshing Ollama models…".into());
        if let Err(error) = thread::Builder::new()
            .name("studio-model-refresh".into())
            .spawn(move || {
                let result = list_ollama_models(&endpoint);
                let _ = tx.send(Update::Installed(result));
            })
        {
            self.updates = None;
            self.busy = None;
            self.message = Some(format!("Imported, but model list refresh failed: {error}"));
        }
    }

    pub fn take_endpoint_for_runs(&mut self) -> Option<String> {
        std::mem::take(&mut self.use_endpoint_for_runs).then(|| endpoint(&self.ollama_endpoint))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_success_is_not_replaced_by_channel_disconnect() {
        let (tx, rx) = mpsc::channel();
        tx.send(Update::Repository(Ok(HubRepository {
            id: "owner/repo".into(),
            sha: "0123456789abcdef".into(),
            siblings: vec![HubFile {
                rfilename: "model.gguf".into(),
                size: Some(1),
            }],
        })))
        .unwrap();
        drop(tx);

        let mut manager = ModelManager {
            updates: Some(rx),
            busy: Some("Loading repository files…".into()),
            ..Default::default()
        };

        manager.receive_updates();

        assert_eq!(
            manager.message.as_deref(),
            Some("Repository metadata loaded.")
        );
        assert_eq!(manager.selected_file.as_deref(), Some("model.gguf"));
        assert!(manager.repository.is_some());
        assert!(manager.busy.is_none());
        assert!(manager.updates.is_none());
    }

    #[test]
    fn repository_url_has_no_empty_path_segment() {
        let url = repository_url("LiquidAI/LFM2.5-1.2B-Instruct-GGUF").unwrap();

        assert_eq!(
            url.as_str(),
            "https://huggingface.co/api/models/LiquidAI/LFM2.5-1.2B-Instruct-GGUF?blobs=true"
        );
    }
}

fn search_hugging_face(query: &str, token: &str) -> Result<Vec<HubModel>, String> {
    let client = http_client()?;
    let mut request = client.get(format!("{HF_API}/models")).query(&[
        ("search", query),
        ("limit", "50"),
        ("sort", "downloads"),
        ("direction", "-1"),
        ("filter", "gguf"),
    ]);
    request = with_hf_auth(request, token);
    let models: Vec<HubModel> = request
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .map_err(|error| error.to_string())?;
    Ok(models
        .into_iter()
        .filter(|model| {
            model
                .tags
                .iter()
                .any(|tag| tag.eq_ignore_ascii_case("gguf"))
        })
        .collect())
}

fn repository_details(id: &str, token: &str) -> Result<HubRepository, String> {
    let client = http_client()?;
    let url = repository_url(id)?;
    with_hf_auth(client.get(url), token)
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .map_err(|error| error.to_string())
}

fn repository_url(id: &str) -> Result<reqwest::Url, String> {
    if !valid_repository_id(id) {
        return Err("Hugging Face returned an invalid repository id".into());
    }
    let mut url =
        reqwest::Url::parse(&format!("{HF_API}/models")).map_err(|error| error.to_string())?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| "Could not construct Hugging Face model URL")?;
        for segment in id.split('/') {
            segments.push(segment);
        }
    }
    url.query_pairs_mut().append_pair("blobs", "true");
    Ok(url)
}

fn list_ollama_models(endpoint: &str) -> Result<LocalModels, String> {
    let client = http_client()?;
    let installed = client
        .get(format!("{endpoint}/api/tags"))
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json::<OllamaTags>()
        .map(|response| response.models)
        .map_err(|error| error.to_string())?;
    let running = client
        .get(format!("{endpoint}/api/ps"))
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json::<OllamaRunning>()
        .map_err(|error| error.to_string())?
        .models
        .into_iter()
        .map(|model| model.name)
        .collect();
    Ok(LocalModels { installed, running })
}

#[derive(Deserialize)]
struct OllamaTags {
    #[serde(default)]
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct OllamaRunning {
    #[serde(default)]
    models: Vec<OllamaModel>,
}

fn load_ollama_model(endpoint: &str, name: &str, context: &str) -> Result<(), String> {
    let context = context
        .parse::<u64>()
        .map_err(|_| "Context size must be a whole number of tokens".to_string())?;
    if !(512..=262_144).contains(&context) {
        return Err("Context size must be 512..262144 tokens".into());
    }
    let response = http_client()?
        .post(format!("{endpoint}/api/generate"))
        .json(&serde_json::json!({
            "model": name,
            "prompt": "",
            "stream": false,
            "keep_alive": "24h",
            "options": { "num_ctx": context }
        }))
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let body: serde_json::Value = response.json().map_err(|error| error.to_string())?;
    if let Some(error) = body.get("error").and_then(serde_json::Value::as_str) {
        return Err(error.into());
    }
    Ok(())
}

fn unload_ollama_model(endpoint: &str, name: &str) -> Result<(), String> {
    let response = http_client()?
        .post(format!("{endpoint}/api/generate"))
        .json(&serde_json::json!({
            "model": name,
            "prompt": "",
            "stream": false,
            "keep_alive": 0
        }))
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let body: serde_json::Value = response.json().map_err(|error| error.to_string())?;
    if let Some(error) = body.get("error").and_then(serde_json::Value::as_str) {
        return Err(error.into());
    }
    Ok(())
}

fn import_gguf(
    repo: &HubRepository,
    file_path: &str,
    ollama: &str,
    token: &str,
    updates: &Sender<Update>,
) -> Result<String, String> {
    if repo.sha.is_empty() {
        return Err("Hugging Face did not return a pinned repository revision".into());
    }
    if !valid_repository_id(&repo.id) {
        return Err("Invalid Hugging Face repository id".into());
    }
    if file_path.is_empty()
        || file_path
            .split('/')
            .any(|part| part == ".." || part.is_empty())
    {
        return Err("Invalid model file path".into());
    }
    let file_meta = repo
        .siblings
        .iter()
        .find(|candidate| candidate.rfilename == file_path)
        .ok_or("The selected file is no longer in the repository")?;
    if file_meta
        .size
        .is_some_and(|size| size > 100 * 1024 * 1024 * 1024)
    {
        return Err("Selected GGUF exceeds the 100 GiB import limit".into());
    }

    let client = http_client()?;
    let mut url = reqwest::Url::parse("https://huggingface.co/").map_err(|e| e.to_string())?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| "Could not construct Hugging Face download URL")?;
        for segment in repo.id.split('/') {
            segments.push(segment);
        }
        segments.push("resolve").push(&repo.sha);
        for segment in file_path.split('/') {
            segments.push(segment);
        }
    }
    url.query_pairs_mut().append_pair("download", "true");
    let response = with_hf_auth(client.get(url), token)
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let total = response.content_length().or(file_meta.size);
    let mut temp = tempfile::NamedTempFile::new().map_err(|error| error.to_string())?;
    let mut response = response;
    let mut hash = Sha256::new();
    let mut written = 0u64;
    let mut buffer = vec![0u8; 4 * 1024 * 1024];
    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|error| format!("Hugging Face download failed: {error}"))?;
        if read == 0 {
            break;
        }
        if written.saturating_add(read as u64) > 100 * 1024 * 1024 * 1024 {
            return Err("Downloaded GGUF exceeds the 100 GiB import limit".into());
        }
        temp.write_all(&buffer[..read])
            .map_err(|error| error.to_string())?;
        hash.update(&buffer[..read]);
        written += read as u64;
        let progress = match total {
            Some(total) if total > 0 => format!(
                "Downloading {}: {} / {}",
                file_path,
                format_bytes(written),
                format_bytes(total)
            ),
            _ => format!("Downloading {}: {}", file_path, format_bytes(written)),
        };
        let _ = updates.send(Update::Progress(progress));
    }
    if written == 0 {
        return Err("Hugging Face returned an empty model file".into());
    }
    if total.is_some_and(|expected| expected != written) {
        return Err(format!(
            "Download size mismatch: expected {}, received {}",
            format_bytes(total.unwrap_or_default()),
            format_bytes(written)
        ));
    }

    let digest = format!("sha256:{:x}", hash.finalize());
    let blob_url = format!("{ollama}/api/blobs/{digest}");
    let mut blob = temp.reopen().map_err(|error| error.to_string())?;
    blob.seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    let _ = updates.send(Update::Progress("Sending model file to Ollama…".into()));
    client
        .post(blob_url)
        .header(reqwest::header::CONTENT_LENGTH, written)
        .body(reqwest::blocking::Body::new(blob))
        .send()
        .map_err(|error| format!("Could not send model to Ollama: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Ollama rejected model file: {error}"))?;

    let name = ollama_name(&repo.id, &repo.sha, file_path);
    let mut files = BTreeMap::new();
    files.insert(file_path.to_owned(), digest);
    let _ = updates.send(Update::Progress("Registering model in Ollama…".into()));
    let mut create = client
        .post(format!("{ollama}/api/create"))
        .json(&serde_json::json!({ "model": name, "files": files, "stream": false }))
        .send()
        .map_err(|error| format!("Could not create Ollama model: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Ollama could not create model: {error}"))?;
    let mut create_output = String::new();
    create
        .read_to_string(&mut create_output)
        .map_err(|error| error.to_string())?;
    if create_output.contains("\"error\"") {
        return Err(format!("Ollama model creation failed: {create_output}"));
    }
    Ok(name)
}

fn ollama_name(repo: &str, revision: &str, file: &str) -> String {
    let source = format!("{repo}-{file}").to_ascii_lowercase();
    let stem = source
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .take(5)
        .collect::<Vec<_>>()
        .join("-");
    let mut digest = Sha256::new();
    digest.update(repo.as_bytes());
    digest.update([0]);
    digest.update(revision.as_bytes());
    digest.update([0]);
    digest.update(file.as_bytes());
    let digest = format!("{:x}", digest.finalize());
    let stem: String = stem.chars().take(48).collect();
    format!("hf-{stem}-{}", &digest[..8])
}

fn short_revision(revision: &str) -> &str {
    revision.get(..revision.len().min(12)).unwrap_or(revision)
}

fn valid_repository_id(id: &str) -> bool {
    let mut segments = id.split('/');
    let safe_segment = |segment: &str| {
        !segment.is_empty()
            && segment
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    };
    segments.next().is_some_and(safe_segment)
        && segments.next().is_some_and(safe_segment)
        && segments.next().is_none()
}

fn endpoint(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    trimmed.strip_suffix("/api").unwrap_or(trimmed).to_owned()
}

fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("Camel-Studio/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(60 * 60))
        .build()
        .map_err(|error| error.to_string())
}

fn with_hf_auth(
    request: reqwest::blocking::RequestBuilder,
    token: &str,
) -> reqwest::blocking::RequestBuilder {
    if token.is_empty() {
        request
    } else {
        request.bearer_auth(token)
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
