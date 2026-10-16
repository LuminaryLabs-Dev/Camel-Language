use crate::{
    document::{self, Document},
    editor,
    library_manager::LibraryManager,
    model_manager::ModelManager,
    runner::{Runner, Update},
    worker::Message,
};
use camel_audio_runtime::language::{
    analyze_buffer, AnalysisResult, CompiledProgram, Declaration, Diagnostic, Error, ExecutionEvent,
};
use eframe::egui::{self, Color32, FontId, Key, Modifiers, RichText};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

type CheckResult = (u64, AnalysisResult);

struct Argument {
    name: String,
    ty: String,
    text: String,
    default: Option<String>,
    use_default: bool,
}
struct Tab {
    id: u64,
    doc: Document,
    revision: u64,
    checked: Option<u64>,
    changed: Instant,
    analysis: Vec<Declaration>,
    diagnostics: Vec<Diagnostic>,
    error: Option<Error>,
    checking: Option<Receiver<CheckResult>>,
    workflow: String,
    arguments: Vec<Argument>,
    jump: Option<(usize, usize)>,
    cursor: (usize, usize),
}
impl Tab {
    fn new(id: u64, doc: Document) -> Self {
        Self {
            id,
            doc,
            revision: 0,
            checked: None,
            changed: Instant::now() - Duration::from_secs(1),
            analysis: Vec::new(),
            diagnostics: Vec::new(),
            error: None,
            checking: None,
            workflow: "Main".into(),
            arguments: Vec::new(),
            jump: None,
            cursor: (1, 1),
        }
    }
    fn select_workflow(&mut self, name: String) {
        self.workflow = name;
        let params = self
            .analysis
            .iter()
            .find(|d| d.kind == "workflow" && d.name == self.workflow)
            .map(|d| d.parameters.clone())
            .unwrap_or_default();
        let old = std::mem::take(&mut self.arguments);
        self.arguments = params
            .into_iter()
            .map(|p| {
                let previous = old.iter().find(|a| a.name == p.name && a.ty == p.ty);
                let literal = p.default.as_ref().and_then(|d| {
                    if p.ty == "string" {
                        serde_json::from_str::<String>(d).ok()
                    } else {
                        serde_json::from_str::<serde_json::Value>(d)
                            .ok()
                            .map(|_| d.clone())
                    }
                });
                Argument {
                    name: p.name,
                    ty: p.ty,
                    text: previous
                        .map(|a| a.text.clone())
                        .unwrap_or_else(|| literal.clone().unwrap_or_default()),
                    default: p.default,
                    use_default: previous.map(|a| a.use_default).unwrap_or(p.optional),
                }
            })
            .collect();
    }
    fn raw_arguments(&self) -> Result<Vec<String>, String> {
        let count = self
            .arguments
            .iter()
            .rposition(|a| !a.use_default)
            .map(|n| n + 1)
            .unwrap_or(0);
        self.arguments[..count]
            .iter()
            .map(|a| {
                if a.use_default {
                    let d = a.default.as_deref().ok_or("Missing default")?;
                    if a.ty == "string" {
                        serde_json::from_str::<String>(d).map_err(|_| {
                            format!("Enter {} explicitly before supplying later inputs.", a.name)
                        })
                    } else if serde_json::from_str::<serde_json::Value>(d).is_ok() {
                        Ok(d.into())
                    } else {
                        Err(format!(
                            "Enter {} explicitly before supplying later parameters",
                            a.name
                        ))
                    }
                } else {
                    Ok(a.text.clone())
                }
            })
            .collect()
    }
    fn invalidate_analysis(&mut self) {
        self.checked = None;
        self.diagnostics.clear();
        self.error = None;
    }
}
#[derive(Clone, Copy)]
enum Action {
    New,
    Open,
    Folder,
    Save,
    SaveAs,
    Check,
    Run,
    Stop,
    FinishRecording,
    Close(usize),
    Quit,
    Refresh,
    About,
}
#[derive(Clone, Copy)]
enum Pending {
    Close(usize),
    Quit,
}
#[derive(Clone, Copy, PartialEq)]
enum OutputTab {
    Timeline,
    Console,
    Result,
    Problems,
}
struct Row {
    id: u64,
    kind: String,
    name: String,
    detail: String,
    started: Instant,
    outcome: Option<(bool, u64)>,
}
pub struct Studio {
    #[cfg(feature = "capture")]
    capture: crate::capture::Capture,
    root: PathBuf,
    files: Vec<PathBuf>,
    tabs: Vec<Tab>,
    active: usize,
    next_id: u64,
    runner: Option<Runner>,
    rows: Vec<Row>,
    console: VecDeque<String>,
    result: Option<String>,
    run_error: Option<Error>,
    run_name: String,
    run_status: String,
    output: OutputTab,
    endpoint: String,
    library_manager: LibraryManager,
    model_manager: ModelManager,
    notice: Option<String>,
    pending: Option<Pending>,
    allow_close: bool,
    about: bool,
    find_open: bool,
    find_focus: bool,
    find: String,
}
impl Studio {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        configure(&cc.egui_ctx);
        let mut app = Self::empty(discover_root(initial.as_deref()));
        if let Some(path) = initial {
            app.open(&path);
        }
        if app.tabs.is_empty() {
            app.new_document();
        }
        app
    }
    fn empty(root: PathBuf) -> Self {
        Self {
            #[cfg(feature = "capture")]
            capture: Default::default(),
            files: document::scripts(&root),
            root,
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
            runner: None,
            rows: Vec::new(),
            console: VecDeque::new(),
            result: None,
            run_error: None,
            run_name: "No run yet".into(),
            run_status: "Ready".into(),
            output: OutputTab::Timeline,
            endpoint: std::env::var("CAMEL_LM_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:1234".into()),
            library_manager: LibraryManager::default(),
            model_manager: ModelManager::default(),
            notice: None,
            pending: None,
            allow_close: false,
            about: false,
            find_open: false,
            find_focus: false,
            find: String::new(),
        }
    }
    fn new_document(&mut self) {
        let doc = Document::new(
            "// Camel workflow\nworkflow void Main() {\n    log(\"Hello from Camel\");\n}\n".into(),
        );
        self.tabs.push(Tab::new(self.next_id, doc));
        self.next_id += 1;
        self.active = self.tabs.len() - 1;
    }
    fn open(&mut self, path: &Path) {
        match Document::open(path) {
            Ok(doc) => {
                if let Some(i) = self.tabs.iter().position(|t| t.doc.path == doc.path) {
                    self.active = i;
                    return;
                }
                self.tabs.push(Tab::new(self.next_id, doc));
                self.next_id += 1;
                self.active = self.tabs.len() - 1;
            }
            Err(e) => self.notice = Some(e),
        }
    }
    fn save(&mut self, index: usize, save_as: bool) -> bool {
        let existing = self.tabs[index].doc.path.clone();
        let target = if save_as || existing.is_none() {
            rfd::FileDialog::new()
                .add_filter("Camel script", &["camel"])
                .set_directory(&self.root)
                .set_file_name(self.tabs[index].doc.name())
                .save_file()
        } else {
            existing
        };
        let Some(mut target) = target else {
            return false;
        };
        if target.extension().is_none() {
            target.set_extension("camel");
        }
        match self.tabs[index].doc.save(&target) {
            Ok(()) => {
                self.files = document::scripts(&self.root);
                self.tabs[index].invalidate_analysis();
                self.tabs[index].revision += 1;
                true
            }
            Err(e) => {
                self.notice = Some(e);
                false
            }
        }
    }
    fn analyze(&mut self, force: bool) {
        for (index, tab) in self.tabs.iter_mut().enumerate() {
            if let Some(rx) = &tab.checking {
                match rx.try_recv() {
                    Ok((revision, result)) => {
                        tab.checking = None;
                        if revision == tab.revision {
                            tab.checked = Some(revision);
                            tab.analysis = result.declarations().to_vec();
                            tab.diagnostics = result.diagnostics().to_vec();
                            tab.error = tab.diagnostics.first().map(Diagnostic::to_error);
                            if tab.error.is_none() && result.is_valid() {
                                let name = if tab
                                    .analysis
                                    .iter()
                                    .any(|d| d.kind == "workflow" && d.name == tab.workflow)
                                {
                                    tab.workflow.clone()
                                } else {
                                    tab.analysis
                                        .iter()
                                        .find(|d| d.kind == "workflow")
                                        .map(|d| d.name.clone())
                                        .unwrap_or_default()
                                };
                                tab.select_workflow(name);
                            } else {
                                tab.analysis.clear();
                            }
                        }
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        tab.checking = None;
                        self.notice = Some("Source checker stopped unexpectedly".into());
                    }
                    Err(_) => {}
                }
            }
            if tab.checking.is_none()
                && tab.checked != Some(tab.revision)
                && (force && index == self.active
                    || tab.changed.elapsed() > Duration::from_millis(400))
            {
                let path = tab
                    .doc
                    .path
                    .clone()
                    .unwrap_or_else(|| self.root.join("Untitled.camel"));
                let source = tab.doc.text.clone();
                let revision = tab.revision;
                let (tx, rx) = mpsc::channel();
                match std::thread::Builder::new()
                    .name("studio-check".into())
                    .spawn(move || {
                        let result = analyze_buffer(&path, &source);
                        let _ = tx.send((revision, result));
                    }) {
                    Ok(_) => tab.checking = Some(rx),
                    Err(e) => self.notice = Some(e.to_string()),
                }
            }
        }
    }
    fn run(&mut self) {
        if self.runner.is_some() {
            return;
        }
        let index = self.active;
        if self.tabs[index].checked != Some(self.tabs[index].revision)
            || self.tabs[index].error.is_some()
        {
            self.notice = Some(
                "Wait for the current source check, and fix any problems before running.".into(),
            );
            return;
        }
        if self.tabs[index].doc.dirty() && !self.save(index, false) {
            return;
        }
        if self
            .tabs
            .iter()
            .enumerate()
            .any(|(i, t)| i != index && t.doc.path.is_some() && t.doc.dirty())
        {
            self.notice = Some(
                "Save other edited scripts before running; imported modules are read from disk."
                    .into(),
            );
            return;
        }
        let tab = &self.tabs[index];
        let Some(path) = tab.doc.path.clone() else {
            return;
        };
        if !tab.doc.unchanged_on_disk() {
            self.notice=Some("Script changed on disk. Reopen it before running; Studio will not execute a different source than the editor.".into());
            return;
        }
        let raw = match tab.raw_arguments() {
            Ok(a) => a,
            Err(e) => {
                self.notice = Some(e);
                return;
            }
        };
        let entry = tab.workflow.clone();
        // Validate the exact saved entry before replacing any prior run output.
        if let Err(e) =
            CompiledProgram::load(&path).and_then(|p| p.parse_arguments(&entry, &raw).map(|_| ()))
        {
            self.notice = Some(e.to_string());
            return;
        }
        let executable = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                self.notice = Some(e.to_string());
                return;
            }
        };
        match Runner::start(&executable, &path, &entry, &raw, &self.endpoint) {
            Ok(runner) => {
                self.runner = Some(runner);
                self.rows.clear();
                self.console.clear();
                self.result = None;
                self.run_error = None;
                let workflow_file = tab
                    .doc
                    .path
                    .as_deref()
                    .and_then(Path::file_stem)
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Workflow".into());
                self.run_name = format!("{workflow_file} / {entry}");
                self.run_status = "Running".into();
                self.output = OutputTab::Timeline;
            }
            Err(e) => self.notice = Some(format!("Could not start runtime: {e}")),
        }
    }
    fn push_console(&mut self, text: String) {
        if self.console.len() >= 2000 {
            self.console.pop_front();
        }
        let text = if text.chars().count() > 16_384 {
            format!(
                "{} … [line truncated]",
                text.chars().take(16_384).collect::<String>()
            )
        } else {
            text
        };
        self.console.push_back(text);
    }
    fn poll(&mut self) {
        let Some(runner) = self.runner.as_mut() else {
            return;
        };
        let stopped = runner.stopped;
        match runner.poll() {
            Ok((updates, done)) => {
                for update in updates {
                    match update {
                        Update::Message(Message::Event { data }) => match data {
                            ExecutionEvent::Started {
                                id,
                                kind,
                                name,
                                detail,
                                site: _,
                            } => {
                                if self.rows.len() < 2000 {
                                    self.rows.push(Row {
                                        id,
                                        kind,
                                        name,
                                        detail,
                                        started: Instant::now(),
                                        outcome: None,
                                    });
                                }
                            }
                            ExecutionEvent::Finished {
                                id,
                                success,
                                elapsed_ms,
                            } => {
                                if let Some(row) = self.rows.iter_mut().find(|r| r.id == id) {
                                    row.outcome = Some((success, elapsed_ms));
                                }
                            }
                            ExecutionEvent::Log { text } => self.push_console(text),
                        },
                        Update::Message(Message::Result { value }) => {
                            self.result =
                                Some(serde_json::to_string_pretty(&value).unwrap_or_default())
                        }
                        Update::Message(Message::Diagnostic { error }) => {
                            self.push_console(error.to_string());
                            self.run_error = Some(error);
                        }
                        Update::Console(text) => self.push_console(text),
                        Update::Closed => {}
                    }
                }
                if let Some(success) = done {
                    self.runner = None;
                    self.run_status = if stopped {
                        "Stopped"
                    } else if success && self.result.is_some() {
                        "Completed"
                    } else {
                        "Failed"
                    }
                    .into();
                    if !success && !stopped && self.run_error.is_none() {
                        self.push_console(
                            "Worker exited without a successful result; see Console.".into(),
                        );
                    }
                }
            }
            Err(e) => {
                self.notice = Some(e);
                self.runner = None;
                self.run_status = "Failed".into();
            }
        }
    }
    fn action(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::New => self.new_document(),
            Action::Open => {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("Camel script", &["camel"])
                    .set_directory(&self.root)
                    .pick_file()
                {
                    self.open(&p);
                }
            }
            Action::Folder => {
                if let Some(p) = rfd::FileDialog::new()
                    .set_directory(&self.root)
                    .pick_folder()
                {
                    self.root = p;
                    self.files = document::scripts(&self.root);
                }
            }
            Action::Save => {
                self.save(self.active, false);
            }
            Action::SaveAs => {
                self.save(self.active, true);
            }
            Action::Check => {
                self.tabs[self.active].invalidate_analysis();
                self.analyze(true);
                self.output = OutputTab::Problems;
            }
            Action::Run => self.run(),
            Action::Stop => {
                if let Some(r) = &mut self.runner {
                    if let Err(e) = r.stop() {
                        self.notice = Some(e);
                    }
                }
            }
            Action::FinishRecording => {
                if let Some(r) = &mut self.runner {
                    if let Err(e) = r.finish_recording() {
                        self.notice = Some(e);
                    }
                }
            }
            Action::Refresh => self.files = document::scripts(&self.root),
            Action::About => self.about = true,
            Action::Close(i) => {
                if self.tabs[i].doc.dirty() {
                    self.pending = Some(Pending::Close(i));
                } else {
                    self.close_tab(i);
                }
            }
            Action::Quit => {
                if self.tabs.iter().any(|t| t.doc.dirty()) || self.runner.is_some() {
                    self.pending = Some(Pending::Quit);
                } else {
                    self.allow_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }
    fn close_tab(&mut self, index: usize) {
        self.tabs.remove(index);
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
        if self.tabs.is_empty() {
            self.new_document();
        }
    }
    fn shortcuts(&mut self, ctx: &egui::Context) -> Option<Action> {
        ctx.input_mut(|i| {
            for (mods, key, action) in [
                (Modifiers::CTRL, Key::N, Action::New),
                (Modifiers::CTRL, Key::O, Action::Open),
                (Modifiers::CTRL | Modifiers::SHIFT, Key::S, Action::SaveAs),
                (Modifiers::CTRL, Key::S, Action::Save),
                (Modifiers::CTRL, Key::F5, Action::Check),
                (Modifiers::SHIFT, Key::F5, Action::Stop),
                (Modifiers::NONE, Key::F5, Action::Run),
            ] {
                if i.consume_key(mods, key) {
                    return Some(action);
                }
            }
            if i.consume_key(Modifiers::NONE, Key::Escape) {
                self.find_open = false;
            }
            None
        })
    }
    fn menu(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                for (text, a) in [
                    ("New workflow    Ctrl+N", Action::New),
                    ("Open workflow…    Ctrl+O", Action::Open),
                    ("Open folder…", Action::Folder),
                    ("Save    Ctrl+S", Action::Save),
                    ("Save As…    Ctrl+Shift+S", Action::SaveAs),
                    ("Close script", Action::Close(self.active)),
                    ("Exit", Action::Quit),
                ] {
                    if ui.button(text).clicked() {
                        *action = Some(a);
                        ui.close();
                    }
                }
            });
            ui.menu_button("Run", |ui| {
                for (text, a) in [
                    ("Validate workflow    Ctrl+F5", Action::Check),
                    ("Run workflow    F5", Action::Run),
                    ("Stop execution    Shift+F5", Action::Stop),
                ] {
                    if ui.button(text).clicked() {
                        *action = Some(a);
                        ui.close();
                    }
                }
            });
            ui.menu_button("View", |ui| {
                if ui.button("Refresh project files").clicked() {
                    *action = Some(Action::Refresh);
                    ui.close();
                }
                for (name, tab) in [
                    ("Timeline", OutputTab::Timeline),
                    ("Console", OutputTab::Console),
                    ("Result", OutputTab::Result),
                    ("Problems", OutputTab::Problems),
                ] {
                    if ui.button(name).clicked() {
                        self.output = tab;
                        ui.close();
                    }
                }
            });
            ui.menu_button("Tools", |ui| {
                if ui.button("Manage Models…").clicked() {
                    self.model_manager.open();
                    ui.close();
                }
                if ui.button("Manage Libraries…").clicked() {
                    self.library_manager.open();
                    ui.close();
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("About / shortcuts").clicked() {
                    *action = Some(Action::About);
                    ui.close();
                }
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("New").clicked() {
                *action = Some(Action::New);
            }
            if ui.button("Open…").clicked() {
                *action = Some(Action::Open);
            }
            if ui.button("Save").clicked() {
                *action = Some(Action::Save);
            }
            ui.separator();
            if ui
                .button("Models")
                .on_hover_text("Search, import, load, and manage Ollama models")
                .clicked()
            {
                self.model_manager.open();
            }
            ui.separator();
            if ui
                .button("Check")
                .on_hover_text("Validate workflow; never runs agents or records audio")
                .clicked()
            {
                *action = Some(Action::Check);
            }
            let tab = &self.tabs[self.active];
            let ready = tab.checked == Some(tab.revision)
                && tab.error.is_none()
                && !tab.workflow.is_empty()
                && self.runner.is_none();
            if ui
                .add_enabled(
                    ready,
                    egui::Button::new("Run workflow").fill(Color32::from_rgb(220, 231, 243)),
                )
                .clicked()
            {
                *action = Some(Action::Run);
            }
            if ui
                .add_enabled(self.runner.is_some(), egui::Button::new("Stop"))
                .clicked()
            {
                *action = Some(Action::Stop);
            }
            let recording = self.runner.is_some()
                && self
                    .rows
                    .iter()
                    .any(|r| r.kind == "recording" && r.outcome.is_none());
            if recording
                && ui
                    .button("Finish recording")
                    .on_hover_text("Send Enter to a record(0) microphone session")
                    .clicked()
            {
                *action = Some(Action::FinishRecording);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new("CAMEL STUDIO  /  0.2.0 PREVIEW")
                        .size(11.0)
                        .color(Color32::from_rgb(105, 113, 125)),
                );
            });
        });
        if let Some(notice) = self.notice.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(notice).color(Color32::from_rgb(160, 64, 46)));
                if ui.small_button("Dismiss").clicked() {
                    self.notice = None;
                }
            });
        }
    }
    fn project(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("Project");
            if ui.small_button("Refresh").clicked() {
                self.files = document::scripts(&self.root);
            }
        });
        ui.label(
            RichText::new(self.root.file_name().unwrap_or_default().to_string_lossy()).size(12.0),
        )
        .on_hover_text(self.root.display().to_string());
        ui.separator();
        let mut open = None;
        egui::ScrollArea::vertical()
            .id_salt("project_files")
            .max_height(ui.available_height() * 0.57)
            .show(ui, |ui| {
                for path in &self.files {
                    let selected = self.tabs[self.active]
                        .doc
                        .path
                        .as_ref()
                        .is_some_and(|p| p == path);
                    let label = path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .replace(['_', '-'], " ");
                    if ui
                        .selectable_label(selected, RichText::new(label).size(12.0))
                        .clicked()
                    {
                        open = Some(path.clone());
                    }
                }
                if self.files.is_empty() {
                    ui.weak("No project scripts found");
                }
            });
        ui.separator();
        ui.strong("Outline");
        egui::ScrollArea::vertical()
            .id_salt("outline")
            .show(ui, |ui| {
                for declaration in &self.tabs[self.active].analysis {
                    ui.label(format!("{}  {}", declaration.kind, declaration.name));
                }
            });
        if let Some(path) = open {
            self.open(&path);
        }
    }
    fn inspector(&mut self, ui: &mut egui::Ui) {
        ui.strong("Workflow setup");
        ui.label(RichText::new("Inputs and components").size(11.0).weak());
        ui.separator();
        egui::ScrollArea::vertical().id_salt("inspector").show(ui,|ui| {
            let tab=&mut self.tabs[self.active];
            let names:Vec<_>=tab.analysis.iter().filter(|d|d.kind=="workflow").map(|d|d.name.clone()).collect();
            ui.strong("Workflow");
            let mut selected=tab.workflow.clone();
            egui::ComboBox::from_id_salt("workflow").width(ui.available_width()-12.0).selected_text(if selected.is_empty(){"No workflow"}else{&selected}).show_ui(ui,|ui|{for name in &names{ui.selectable_value(&mut selected,name.clone(),name);}});
            if selected!=tab.workflow{tab.select_workflow(selected);}
            for a in &mut tab.arguments {
                ui.horizontal(|ui|{ui.label(&a.name);ui.weak(display_type(&a.ty));});
                ui.checkbox(&mut a.use_default,"Use default input");
                if !a.use_default {
                    ui.add(egui::TextEdit::singleline(&mut a.text).desired_width(f32::INFINITY).hint_text(if a.ty=="string"{"Enter text"}else{"Enter a value"}));
                }
            }
            if names.is_empty(){ui.weak("No runnable workflow is available.");}
            ui.add_space(10.0);ui.separator();ui.strong("Model service");
            ui.add(egui::TextEdit::singleline(&mut self.endpoint).desired_width(f32::INFINITY));
            ui.label(RichText::new("Configured address; connection not tested.\nModels are chosen for each agent.").size(11.0).weak());
            for kind in ["agent","tool","policy","record"] {
                let declarations:Vec<_>=tab.analysis.iter().filter(|d|d.kind==kind).collect();
                if declarations.is_empty(){continue;}
                ui.add_space(10.0);ui.separator();ui.strong(match kind{"agent"=>"Agents","tool"=>"Integrations","policy"=>"Safety rules",_=>"Data types"});
                for d in declarations {
                    egui::CollapsingHeader::new(&d.name).id_salt((&d.kind,&d.name)).default_open(kind=="agent").show(ui,|ui|{
                        for (key,value) in &d.details {
                            ui.label(RichText::new(display_detail_name(key)).size(11.0).weak());ui.label(RichText::new(display_detail_value(value)).size(12.0));ui.add_space(3.0);
                        }
                    });
                }
            }
            ui.add_space(14.0);ui.label(RichText::new("Safety rules constrain agent actions. Workflows run with the permissions of this computer.").size(11.0).weak());
        });
    }
    fn output(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (name, tab) in [
                ("Timeline", OutputTab::Timeline),
                ("Console", OutputTab::Console),
                ("Result", OutputTab::Result),
                ("Problems", OutputTab::Problems),
            ] {
                ui.selectable_value(&mut self.output, tab, name);
            }
            ui.separator();
            ui.label(RichText::new(&self.run_status).strong());
            ui.weak(&self.run_name);
        });
        ui.separator();
        egui::ScrollArea::both().id_salt("run_output").auto_shrink([false,false]).show(ui,|ui|{match self.output {
            OutputTab::Timeline=>{
                if self.rows.is_empty(){ui.add_space(14.0);ui.weak("Run a workflow to inspect actual agent, model, tool, and microphone activity.");}
                else {
                    egui::Grid::new("timeline_grid").num_columns(5).striped(true).spacing([20.0,7.0]).show(ui,|ui| {
                        for title in ["STATE","KIND","OPERATION","DETAIL","TIME"]{ui.label(RichText::new(title).size(10.0).weak());}ui.end_row();
                        for row in &self.rows {
                            let (state,color)=match row.outcome {Some((true,_))=>("Done",Color32::from_rgb(48,112,73)),Some((false,_))=>("Failed",Color32::from_rgb(160,64,46)),None if self.runner.is_none()=>("Stopped",Color32::from_rgb(112,116,124)),None=>("Running",Color32::from_rgb(46,91,147))};
                            ui.colored_label(color,state);ui.label(&row.kind);
                            ui.label(&row.name);
                            ui.label(&row.detail);let ms=row.outcome.map(|(_,ms)|ms).unwrap_or(row.started.elapsed().as_millis() as u64);ui.monospace(format!("{:.2}s",ms as f64/1000.0));ui.end_row();
                        }
                    });
                }
            },
            OutputTab::Console=>{let mut text=self.console.iter().cloned().collect::<Vec<_>>().join("\n");if text.is_empty(){ui.weak("Script logs and runtime diagnostics appear here.");}else{ui.add(egui::TextEdit::multiline(&mut text).font(FontId::monospace(12.0)).desired_width(f32::INFINITY).interactive(false).frame(false));}},
            OutputTab::Result=>if let Some(result)=&self.result {let mut text=result.clone();ui.add(egui::TextEdit::multiline(&mut text).font(FontId::monospace(12.0)).desired_width(f32::INFINITY).interactive(false).frame(false));if ui.button("Copy JSON").clicked(){ui.ctx().copy_text(result.clone());}}else{ui.weak("The completed workflow's return value appears here as JSON.");},
            OutputTab::Problems=>{
                let tab=&self.tabs[self.active];
                if !tab.diagnostics.is_empty() {
                    ui.label(RichText::new("Current script").strong());
                    for diagnostic in &tab.diagnostics {
                        let site = &diagnostic.primary_span.start;
                        ui.label(format!(
                            "{:?} [{}]  {}:{}:{}: {}",
                            diagnostic.severity,
                            diagnostic.code,
                            site.file,
                            site.line,
                            site.column,
                            diagnostic.message
                        ));
                        if let Some(help) = &diagnostic.help {
                            ui.weak(help);
                        }
                    }
                } else if tab.checked==Some(tab.revision) {
                    ui.label("Workflow is ready to run.");
                } else if tab.error.is_none() {
                    ui.weak("Preparing workflow…");
                }
                if self.run_error.is_some() {
                    ui.label(RichText::new("Last run").strong());
                    ui.label("Needs attention. Review the selected workflow and try again.");
                }
            },
        }});
    }
    fn center(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        self.source_editor(ui, action);
    }
    fn source_editor(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        egui::ScrollArea::horizontal()
            .id_salt("document_tabs")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (index, tab) in self.tabs.iter().enumerate() {
                        let label = format!(
                            "{}{}",
                            tab.doc.name(),
                            if tab.doc.dirty() { " *" } else { "" }
                        );
                        if ui.selectable_label(self.active == index, label).clicked() {
                            self.active = index;
                        }
                        if ui.small_button("×").on_hover_text("Close script").clicked() {
                            *action = Some(Action::Close(index));
                        }
                    }
                });
            });
        ui.separator();
        if self.find_open {
            let mut next = false;
            ui.horizontal(|ui| {
                ui.label("Find:");
                let response =
                    ui.add(egui::TextEdit::singleline(&mut self.find).desired_width(220.0));
                if self.find_focus {
                    response.request_focus();
                    self.find_focus = false;
                }
                next = ui.button("Next").clicked()
                    || response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                if ui.small_button("Close").clicked() {
                    self.find_open = false;
                }
            });
            if next && !self.find.is_empty() {
                let tab = &mut self.tabs[self.active];
                let from = editor::character_at(&tab.doc.text, tab.cursor.0, tab.cursor.1);
                let bytes = tab
                    .doc
                    .text
                    .char_indices()
                    .nth(from + 1)
                    .map(|(i, _)| i)
                    .unwrap_or(tab.doc.text.len());
                let found = tab.doc.text[bytes..]
                    .find(&self.find)
                    .map(|i| i + bytes)
                    .or_else(|| tab.doc.text.find(&self.find));
                if let Some(i) = found {
                    let prefix = &tab.doc.text[..i];
                    tab.jump = Some((
                        prefix.bytes().filter(|b| *b == b'\n').count() + 1,
                        prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1,
                    ));
                } else {
                    self.notice = Some("No matching text".into());
                }
            }
            ui.separator();
        }
        let tab = &mut self.tabs[self.active];
        let (changed, cursor) = editor::show(
            ui,
            &mut tab.doc.text,
            egui::Id::new(("editor", tab.id)),
            tab.jump.take(),
        );
        tab.cursor = cursor;
        if changed {
            tab.invalidate_analysis();
            tab.revision += 1;
            tab.changed = Instant::now();
        }
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.about {
            egui::Window::new("About Camel Studio").collapsible(false).resizable(false).open(&mut self.about).show(ctx,|ui|{
                ui.heading("Camel Studio");ui.label("0.2.0 preview • native Camel code editor");ui.separator();
                ui.label("Edit a script, configure its inputs, and inspect workflow results.");ui.label("Ctrl+O: open script\nCtrl+F5: validate\nF5: run • Shift+F5: stop");ui.separator();
                ui.label("Stop ends the active run. Completed file writes are not undone.\nIt does not unload models or stop LM Studio.\nFinish recording ends an open capture.\nValidate checks setup without starting a run. Workflows use this computer's permissions.");
            });
        }
        let Some(pending) = self.pending else {
            return;
        };
        let mut choice = None;
        egui::Modal::new(egui::Id::new("unsaved_changes")).show(ctx, |ui| {
            ui.heading(match pending {
                Pending::Quit => "Close Camel Studio?",
                Pending::Close(_) => "Close script?",
            });
            ui.label("Unsaved edits will be lost if discarded.");
            if let Some(notice) = &self.notice {
                ui.colored_label(Color32::from_rgb(160, 64, 46), notice);
            }
            if self.runner.is_some() && matches!(pending, Pending::Quit) {
                ui.label("Closing also stops the active worker; prior side effects remain.");
            }
            ui.horizontal(|ui| {
                if ui.button("Save and close").clicked() {
                    choice = Some(0);
                }
                if ui.button("Discard and close").clicked() {
                    choice = Some(1);
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(2);
                }
            });
        });
        if let Some(choice) = choice {
            if choice == 2 {
                self.pending = None;
                return;
            }
            if choice == 0 {
                let indices: Vec<usize> = match pending {
                    Pending::Close(i) => vec![i],
                    Pending::Quit => self
                        .tabs
                        .iter()
                        .enumerate()
                        .filter(|(_, t)| t.doc.dirty())
                        .map(|(i, _)| i)
                        .collect(),
                };
                for i in indices {
                    if self.tabs[i].doc.dirty() && !self.save(i, false) {
                        return;
                    }
                }
            }
            self.pending = None;
            match pending {
                Pending::Close(i) => self.close_tab(i),
                Pending::Quit => {
                    self.runner = None;
                    self.allow_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }
    pub fn draw(&mut self, ctx: &egui::Context) {
        self.poll();
        self.analyze(false);
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.action(Action::Quit, ctx);
        }
        let mut action = if self.pending.is_none() {
            self.shortcuts(ctx)
        } else {
            None
        };
        egui::TopBottomPanel::top("menus").show(ctx, |ui| self.menu(ui, &mut action));
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            let tab = &self.tabs[self.active];
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(if tab.checked != Some(tab.revision) {
                        "Preparing…"
                    } else if tab.error.is_some() {
                        "Needs attention"
                    } else {
                        "Ready"
                    })
                    .size(11.0),
                );
            });
        });
        egui::TopBottomPanel::bottom("runs")
            .resizable(true)
            .default_height(210.0)
            .min_height(110.0)
            .show(ctx, |ui| self.output(ui));
        egui::SidePanel::left("project")
            .resizable(true)
            .default_width(215.0)
            .width_range(160.0..=360.0)
            .show(ctx, |ui| self.project(ui));
        egui::SidePanel::right("orchestration")
            .resizable(true)
            .default_width(285.0)
            .width_range(240.0..=450.0)
            .show(ctx, |ui| self.inspector(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).fill(Color32::WHITE))
            .show(ctx, |ui| self.center(ui, &mut action));
        if self.pending.is_none() {
            if let Some(a) = action {
                self.action(a, ctx);
            }
        }
        self.dialogs(ctx);
        self.library_manager.show(ctx);
        self.model_manager.show(ctx);
        if let Some(endpoint) = self.model_manager.take_endpoint_for_runs() {
            self.endpoint = endpoint;
            self.notice = Some("Ollama is selected for the next workflow run.".into());
        }
        let tab = &self.tabs[self.active];
        let workflow_title = tab
            .doc
            .path
            .as_deref()
            .and_then(Path::file_stem)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Workflow".into());
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "{workflow_title} — Camel Studio{}",
            if tab.doc.dirty() { " *" } else { "" }
        )));
        if self.runner.is_some() || self.tabs.iter().any(|t| t.checked != Some(t.revision)) {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        #[cfg(feature = "capture")]
        if self.capture.active() {
            let checked = self.tabs[self.active].checked == Some(self.tabs[self.active].revision);
            if self.capture.run_pending && checked {
                self.capture.run_pending = false;
                self.run();
            }
            if self.capture.tick(
                ctx,
                checked && !self.capture.run_pending && self.runner.is_none(),
            ) {
                self.allow_close = true;
            }
        }
    }
}
impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.draw(ctx);
    }
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.runner = None;
    }
}
fn discover_root(initial: Option<&Path>) -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let exe = std::env::current_exe().ok();
    if let Some(path) = initial.and_then(|p| p.canonicalize().ok()) {
        let parent = path.parent().unwrap();
        return parent
            .ancestors()
            .find(|p| p.join("Camel/examples").is_dir())
            .unwrap_or(parent)
            .into();
    }
    for start in [
        exe.as_deref().and_then(Path::parent),
        Some(cwd.as_path()),
        Some(Path::new(env!("CARGO_MANIFEST_DIR"))),
    ]
    .into_iter()
    .flatten()
    {
        for p in start.ancestors() {
            if p.join("Camel/examples").is_dir() {
                return p.canonicalize().unwrap_or_else(|_| p.into());
            }
            if p.join("Camel-Language/Camel/examples").is_dir() {
                let root = p.join("Camel-Language");
                return root.canonicalize().unwrap_or(root);
            }
        }
    }
    initial.and_then(Path::parent).unwrap_or(&cwd).into()
}
fn display_type(ty: &str) -> String {
    if let Some(inner) = ty.strip_suffix("[]") {
        return format!("List of {}", display_type(inner).to_lowercase());
    }
    match ty {
        "string" | "text" => "Text".into(),
        "int" | "integer" => "Whole number".into(),
        "float" | "number" => "Number".into(),
        "bool" | "boolean" => "Yes / no".into(),
        "any" => "Any value".into(),
        _ => "Structured value".into(),
    }
}
fn display_detail_name(name: &str) -> &str {
    match name {
        "model" => "Model",
        "system" => "Instructions",
        "maxTurns" => "Turn limit",
        "maxToolCalls" => "Tool-call limit",
        "allowNetwork" => "Network access",
        "allowFileRead" => "File access",
        "allowFileWrite" => "File changes",
        "allowShell" => "Command access",
        "policy" => "Safety rules",
        "response" => "Response format",
        "returns" => "Response format",
        _ => name,
    }
}
fn display_detail_value(value: &str) -> String {
    if value.starts_with("env(") || value.starts_with("<expression") {
        return "Configured automatically".into();
    }
    if value.starts_with('[') {
        return "Configured values".into();
    }
    if let Ok(text) = serde_json::from_str::<String>(value) {
        return text;
    }
    match value {
        "true" => "Enabled".into(),
        "false" => "Disabled".into(),
        "string" | "text" | "int" | "integer" | "float" | "number" | "bool" | "boolean" | "any" => {
            display_type(value)
        }
        _ => value.into(),
    }
}
fn configure(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::light());
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(9.0, 4.0);
    style.visuals.panel_fill = Color32::from_rgb(245, 246, 248);
    style.visuals.window_fill = Color32::from_rgb(250, 250, 251);
    style.visuals.selection.bg_fill = Color32::from_rgb(204, 223, 243);
    style.visuals.selection.stroke = egui::Stroke::new(1.0_f32, Color32::from_rgb(39, 67, 97));
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
    ] {
        widget.corner_radius = egui::CornerRadius::same(3);
    }
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, FontId::proportional(17.0));
    ctx.set_style(style);
    #[cfg(windows)]
    {
        let mut fonts = egui::FontDefinitions::default();
        for (name, path, family) in [
            (
                "Segoe UI",
                r"C:\Windows\Fonts\segoeui.ttf",
                egui::FontFamily::Proportional,
            ),
            (
                "Consolas",
                r"C:\Windows\Fonts\consola.ttf",
                egui::FontFamily::Monospace,
            ),
        ] {
            if let Ok(bytes) = std::fs::read(path) {
                fonts.font_data.insert(
                    name.into(),
                    std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                );
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .insert(0, name.into());
            }
        }
        ctx.set_fonts(fonts);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ui_lays_out_a_real_checked_program() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let mut app = Studio::empty(root.clone());
        app.open(&root.join("Camel/examples/programming_core.camel"));
        let p = CompiledProgram::load(app.tabs[0].doc.path.as_ref().unwrap()).unwrap();
        app.tabs[0].analysis = p.declarations();
        app.tabs[0].checked = Some(0);
        app.tabs[0].select_workflow("Main".into());
        let ctx = egui::Context::default();
        configure(&ctx);
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1320.0, 880.0),
                )),
                ..Default::default()
            },
            |ctx| app.draw(ctx),
        );
        assert!(!out.shapes.is_empty());
        assert_eq!(app.tabs[0].arguments.len(), 1);
        assert!(app.runner.is_none());
    }
    #[test]
    fn default_arguments_are_omitted_not_executed() {
        let p = CompiledProgram::from_source(
            "test.camel",
            "workflow int Main(int x = 3) { return x; }",
        )
        .unwrap();
        let mut tab = Tab::new(1, Document::new(String::new()));
        tab.analysis = p.declarations();
        tab.select_workflow("Main".into());
        assert!(tab.raw_arguments().unwrap().is_empty());
        tab.arguments[0].use_default = false;
        tab.arguments[0].text = "9".into();
        assert_eq!(tab.raw_arguments().unwrap(), vec!["9"]);
    }
}
