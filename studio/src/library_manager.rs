use camel_audio_runtime::libraries::{self, Catalog, Library};
use eframe::egui;
use std::{
    fs,
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    thread,
};

enum Update {
    Catalog(Result<Catalog, String>),
    Operation(Result<String, String>),
}

#[derive(Default)]
pub struct LibraryManager {
    open: bool,
    query: String,
    catalog: Option<Catalog>,
    selected: Option<String>,
    busy: Option<String>,
    message: Option<String>,
    updates: Option<Receiver<Update>>,
}

impl LibraryManager {
    pub fn open(&mut self) {
        self.open = true;
        if self.catalog.is_none() && self.updates.is_none() {
            self.load_catalog(false);
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        self.receive_updates();
        if self.updates.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        if !self.open {
            return;
        }

        let mut open = self.open;
        egui::Window::new("Library Manager")
            .open(&mut open)
            .default_size([900.0, 590.0])
            .min_size([680.0, 420.0])
            .show(ctx, |ui| self.content(ui));
        self.open = open;
    }

    fn content(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Search libraries");
            ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .desired_width(260.0)
                    .hint_text("Name or description"),
            );
            if ui
                .add_enabled(self.busy.is_none(), egui::Button::new("Refresh catalog"))
                .clicked()
            {
                self.load_catalog(true);
            }
            if self.busy.is_some() {
                ui.spinner();
            }
        });
        ui.separator();

        let visible: Vec<Library> = self
            .catalog
            .as_ref()
            .map(|catalog| {
                catalog
                    .libraries
                    .iter()
                    .filter(|library| {
                        self.query.trim().is_empty()
                            || library
                                .name
                                .to_lowercase()
                                .contains(&self.query.to_lowercase())
                            || library
                                .description
                                .to_lowercase()
                                .contains(&self.query.to_lowercase())
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        ui.columns(2, |columns| {
            egui::ScrollArea::vertical()
                .id_salt("library_catalog")
                .show(&mut columns[0], |ui| {
                    if self.catalog.is_none() {
                        if self.updates.is_some() {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Loading library catalog…");
                            });
                        } else {
                            ui.weak("No catalog loaded. Refresh to try again.");
                        }
                    } else if visible.is_empty() {
                        ui.weak("No matching libraries.");
                    }
                    for library in &visible {
                        let installed = installed_library(&library.name);
                        let state = match installed.as_ref() {
                            Some(item) if item.version == library.version => "Installed",
                            Some(_) => "Update available",
                            None => "Available",
                        };
                        let selected = self.selected.as_deref() == Some(&library.name);
                        let response = ui.selectable_label(
                            selected,
                            egui::RichText::new(&library.name).strong(),
                        );
                        if response.clicked() {
                            self.selected = Some(library.name.clone());
                        }
                        ui.horizontal(|ui| {
                            ui.add_space(12.0);
                            ui.label(egui::RichText::new(&library.description).weak());
                        });
                        ui.horizontal(|ui| {
                            ui.add_space(12.0);
                            ui.weak(format!("Version {} · {state}", library.version));
                        });
                        ui.separator();
                    }
                });

            egui::ScrollArea::vertical()
                .id_salt("library_details")
                .show(&mut columns[1], |ui| {
                    let selected = self
                        .selected
                        .as_deref()
                        .and_then(|name| visible.iter().find(|item| item.name == name))
                        .cloned();
                    if let Some(library) = selected {
                        ui.heading(&library.name);
                        ui.label(&library.description);
                        ui.add_space(14.0);
                        ui.label(format!("Catalog version: {}", library.version));
                        ui.label(format!("Repository: {}", library.repository));
                        ui.label(format!("Package files: {}", library.files.len()));
                        if let Some(installed) = installed_library(&library.name) {
                            ui.add_space(8.0);
                            ui.label(format!("Installed version: {}", installed.version));
                            ui.weak(installed.path.display().to_string());
                            ui.horizontal(|ui| {
                                let action = if installed.version == library.version {
                                    "Reinstall"
                                } else {
                                    "Update"
                                };
                                if ui
                                    .add_enabled(
                                        self.busy.is_none(),
                                        egui::Button::new(action),
                                    )
                                    .clicked()
                                {
                                    self.install(library.clone());
                                }
                                if ui
                                    .add_enabled(
                                        self.busy.is_none(),
                                        egui::Button::new("Remove"),
                                    )
                                    .clicked()
                                {
                                    self.remove(library.name.clone());
                                }
                            });
                        } else if ui
                            .add_enabled(self.busy.is_none(), egui::Button::new("Install"))
                            .clicked()
                        {
                            self.install(library);
                        }
                    } else if self.catalog.is_some() {
                        ui.weak("Select a library to see details and manage its installation.");
                    }
                });
        });

        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if let Some(message) = &self.message {
                ui.label(message);
            } else {
                ui.weak(format!("Libraries install under {}", install_root_display()));
            }
        });
    }

    fn load_catalog(&mut self, remote: bool) {
        if self.updates.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.updates = Some(rx);
        self.message = Some(if remote {
            "Refreshing catalog from the library registry…".into()
        } else {
            "Loading the installed catalog…".into()
        });
        if let Err(error) = thread::Builder::new()
            .name("studio-library-catalog".into())
            .spawn(move || {
                let source = remote.then_some(libraries::CATALOG_URL);
                let result = libraries::catalog(source).map_err(|error| error.to_string());
                let _ = tx.send(Update::Catalog(result));
            })
        {
            self.updates = None;
            self.message = Some(format!("Could not start catalog refresh: {error}"));
        }
    }

    fn install(&mut self, library: Library) {
        let (sender, receiver) = mpsc::channel();
        self.updates = Some(receiver);
        let name = library.name.clone();
        self.busy = Some(name.clone());
        self.message = Some(format!("Installing {}…", library.name));
        if let Err(error) = thread::Builder::new()
            .name("studio-library-install".into())
            .spawn(move || {
                let result = libraries::install(&library)
                    .map(|path| format!("Installed {} {} at {}", library.name, library.version, path.display()))
                    .map_err(|error| error.to_string());
                let _ = sender.send(Update::Operation(result));
            })
        {
            self.updates = None;
            self.busy = None;
            self.message = Some(format!("Could not start library installation: {error}"));
        }
    }

    fn remove(&mut self, name: String) {
        let (sender, receiver) = mpsc::channel();
        self.updates = Some(receiver);
        self.busy = Some(name.clone());
        self.message = Some(format!("Removing {name}…"));
        if let Err(error) = thread::Builder::new()
            .name("studio-library-remove".into())
            .spawn(move || {
                let result = libraries::remove(&name)
                    .map(|_| format!("Removed {name}"))
                    .map_err(|error| error.to_string());
                let _ = sender.send(Update::Operation(result));
            })
        {
            self.updates = None;
            self.busy = None;
            self.message = Some(format!("Could not start library removal: {error}"));
        }
    }

    fn receive_updates(&mut self) {
        let Some(receiver) = &self.updates else {
            return;
        };
        let update = match receiver.try_recv() {
            Ok(update) => update,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.updates = None;
                self.busy = None;
                self.message = Some("Library operation stopped unexpectedly.".into());
                return;
            }
        };
        self.updates = None;
        match update {
            Update::Catalog(Ok(catalog)) => {
                if self.selected.as_ref().is_some_and(|name| {
                    !catalog.libraries.iter().any(|item| &item.name == name)
                }) {
                    self.selected = None;
                }
                self.catalog = Some(catalog);
                self.message = Some("Catalog ready.".into());
            }
            Update::Catalog(Err(error)) => self.message = Some(format!("Catalog error: {error}")),
            Update::Operation(result) => {
                self.busy = None;
                self.message = Some(result.unwrap_or_else(|error| format!("Library error: {error}")));
            }
        }
    }
}

struct InstalledLibrary {
    version: String,
    path: PathBuf,
}

fn installed_library(name: &str) -> Option<InstalledLibrary> {
    let path = libraries::installed_library(name)?;
    let version = fs::read(path.join(".camel-library.json"))
        .ok()
        .and_then(|metadata| serde_json::from_slice::<Library>(&metadata).ok())
        .map(|library| library.version)
        .unwrap_or_else(|| "Unknown".into());
    Some(InstalledLibrary {
        version,
        path,
    })
}

fn install_root_display() -> String {
    libraries::installed_root()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "set CAMEL_HOME first".into())
}
