use camel_setup::{
    bundle::Bundle,
    install::{self, Receipt, Request},
};
use eframe::egui::{self, Color32, FontId, RichText, TextStyle};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    time::Duration,
};

enum Update {
    Progress(String),
    Finished(Result<Receipt, String>),
}
enum Page {
    Welcome,
    Location,
    Review,
    Installing(Receiver<Update>),
    Done(Receipt),
    Failed(String),
}
enum Action {
    None,
    Next,
    Back,
    Install,
    Retry,
    Close,
}

pub struct Wizard {
    bundle_root: PathBuf,
    bundle: Result<Bundle, String>,
    destination: String,
    add_to_path: bool,
    page: Page,
    progress: String,
    #[cfg(feature = "capture")]
    frames: usize,
}

impl Wizard {
    pub fn new(cc: &eframe::CreationContext<'_>, bundle_root: PathBuf) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::light();
        style.visuals.panel_fill = Color32::from_rgb(250, 248, 244);
        style.visuals.selection.bg_fill = Color32::from_rgb(212, 169, 118);
        style.spacing.item_spacing = egui::vec2(12.0, 12.0);
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(26.0));
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(16.0));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(16.0));
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(14.0));
        cc.egui_ctx.set_style(style);
        let bundle = Bundle::load(&bundle_root).map_err(|e| e.to_string());
        Self {
            bundle_root,
            bundle,
            destination: install::default_destination()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            add_to_path: true,
            page: Page::Welcome,
            progress: String::new(),
            #[cfg(feature = "capture")]
            frames: 0,
        }
    }

    fn start(&mut self, ctx: &egui::Context) {
        let Ok(bundle) = &self.bundle else {
            return;
        };
        let bundle = bundle.clone();
        let request = Request {
            destination: PathBuf::from(&self.destination),
            add_to_path: self.add_to_path,
        };
        let (sender, receiver) = mpsc::channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = install::install(&bundle, &request, |message| {
                let _ = sender.send(Update::Progress(message.into()));
                repaint.request_repaint();
            })
            .map_err(|e| e.to_string());
            let _ = sender.send(Update::Finished(result));
            repaint.request_repaint();
        });
        self.page = Page::Installing(receiver);
        self.progress = "Preparing installation…".into();
    }

    fn poll(&mut self) {
        let Page::Installing(receiver) = &self.page else {
            return;
        };
        let mut finish = None;
        loop {
            match receiver.try_recv() {
                Ok(Update::Progress(message)) => self.progress = message,
                Ok(Update::Finished(result)) => {
                    finish = Some(result);
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    finish = Some(Err("Setup worker stopped unexpectedly. Review the installation folder before retrying.".into()));
                    break;
                }
            }
        }
        if let Some(result) = finish {
            self.page = match result {
                Ok(receipt) => Page::Done(receipt),
                Err(error) => Page::Failed(error),
            };
        }
    }
}

impl eframe::App for Wizard {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.poll();
        let busy = matches!(self.page, Page::Installing(_));
        if busy && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        let mut action = Action::None;
        egui::SidePanel::left("steps")
            .exact_width(175.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(28.0);
                ui.heading(
                    RichText::new("CAMEL")
                        .color(Color32::from_rgb(113, 72, 30))
                        .size(26.0),
                );
                ui.label("SETUP");
                ui.add_space(38.0);
                let active = match &self.page {
                    Page::Welcome => 0,
                    Page::Location => 1,
                    Page::Review => 2,
                    _ => 3,
                };
                for (i, label) in ["1  Welcome", "2  Location", "3  Review", "4  Install"]
                    .iter()
                    .enumerate()
                {
                    ui.label(RichText::new(*label).strong().color(if i == active {
                        Color32::from_rgb(138, 85, 32)
                    } else {
                        Color32::GRAY
                    }));
                    ui.add_space(10.0);
                }
            });
        egui::TopBottomPanel::bottom("buttons").show(ctx, |ui| {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if matches!(self.page, Page::Location | Page::Review) && ui.button("Back").clicked()
                {
                    action = Action::Back;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    match &self.page {
                        Page::Welcome => {
                            if ui
                                .add_enabled(self.bundle.is_ok(), egui::Button::new("Next"))
                                .clicked()
                            {
                                action = Action::Next;
                            }
                        }
                        Page::Location => {
                            if ui
                                .add_enabled(
                                    !self.destination.trim().is_empty(),
                                    egui::Button::new("Next"),
                                )
                                .clicked()
                            {
                                action = Action::Next;
                            }
                        }
                        Page::Review => {
                            if ui.button("Install Camel").clicked() {
                                action = Action::Install;
                            }
                        }
                        Page::Done(_) => {
                            if ui.button("Finish").clicked() {
                                action = Action::Close;
                            }
                        }
                        Page::Failed(_) => {
                            if ui.button("Try again").clicked() {
                                action = Action::Retry;
                            }
                        }
                        Page::Installing(_) => {
                            ui.add_enabled(false, egui::Button::new("Installing…"));
                        }
                    }
                    if !busy && !matches!(self.page, Page::Done(_)) && ui.button("Cancel").clicked()
                    {
                        action = Action::Close;
                    }
                });
            });
            ui.add_space(12.0);
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(30.0);
            match &self.page {
                Page::Welcome => {
                    ui.heading("Welcome to Camel");
                    ui.add_space(18.0);
                    ui.label("Install Camel for this account and run it from any new terminal.");
                    ui.label("The setup wizard checks the distribution, copies Camel, and adds its commands to PATH.");
                    ui.add_space(18.0);
                    match &self.bundle {
                        Ok(bundle) => {
                            ui.label(format!("Camel {} · {} {}", bundle.manifest.version, bundle.manifest.os, bundle.manifest.arch));
                            ui.label(if bundle.includes_studio() { "Includes Camel CLI and Camel Studio." } else { "Includes Camel CLI." });
                        }
                        Err(error) => { ui.colored_label(Color32::from_rgb(170, 40, 40), error); ui.label("Extract the entire distribution, or choose its folder below."); }
                    }
                    ui.add_space(18.0);
                    if ui.button("Choose distribution folder…").clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            self.bundle_root = folder;
                            self.bundle = Bundle::load(&self.bundle_root).map_err(|e| e.to_string());
                        }
                    }
                }
                Page::Location => {
                    ui.heading("Choose an installation folder");
                    ui.add_space(18.0);
                    ui.label("An existing Camel installation in this folder will be upgraded.");
                    ui.add(egui::TextEdit::singleline(&mut self.destination).desired_width(f32::INFINITY));
                    if ui.button("Browse…").clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() { self.destination = folder.join("Camel").display().to_string(); }
                    }
                    ui.add_space(18.0);
                    ui.checkbox(&mut self.add_to_path, "Add Camel commands to my PATH (recommended)");
                    ui.label("Applies to your account. No administrator password is needed for the default folder.");
                    ui.label("Open a new terminal after setup; existing terminals keep their current environment.");
                }
                Page::Review => {
                    ui.heading("Ready to install");
                    ui.add_space(18.0);
                    ui.label("Installation folder");
                    ui.label(RichText::new(&self.destination).monospace());
                    ui.add_space(18.0);
                    ui.label(if self.add_to_path { "Add Camel to this account's PATH" } else { "PATH will not be updated." });
                    ui.label("Check every installed file against the bundle manifest");
                    ui.label("Verify the Camel command starts successfully");
                    ui.add_space(18.0);
                    ui.label("Your Camel library data is stored separately and is preserved during upgrades.");
                }
                Page::Installing(_) => {
                    ui.heading("Installing Camel");
                    ui.add_space(24.0);
                    ui.spinner();
                    ui.label(&self.progress);
                    ui.label("Please keep this window open until installation completes.");
                    ctx.request_repaint_after(Duration::from_millis(100));
                }
                Page::Done(receipt) => {
                    ui.heading("Camel is installed");
                    ui.add_space(18.0);
                    ui.label(format!("Camel {} is ready in", receipt.version));
                    ui.label(RichText::new(receipt.destination.display().to_string()).monospace());
                    ui.add_space(18.0);
                    if receipt.path_locations.is_empty() { ui.label("Run Camel from the bin folder inside your installation."); }
                    else { ui.label("Open a new terminal and try:"); }
                    ui.code("camel --version");
                    ui.code("camel library list");
                    ui.add_space(18.0);
                    ui.label("If a terminal launched from an existing app cannot find Camel, restart that app or sign out and back in.");
                }
                Page::Failed(error) => {
                    ui.heading("Setup could not finish");
                    ui.add_space(18.0);
                    ui.colored_label(Color32::from_rgb(170, 40, 40), error);
                    ui.label("Check the folder and permissions, then try again.");
                }
            }
        });
        match action {
            Action::Next => {
                self.page = if matches!(self.page, Page::Welcome) {
                    Page::Location
                } else {
                    Page::Review
                }
            }
            Action::Back => {
                self.page = if matches!(self.page, Page::Review) {
                    Page::Location
                } else {
                    Page::Welcome
                }
            }
            Action::Install => self.start(ctx),
            Action::Retry => self.page = Page::Location,
            Action::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Action::None => {}
        }
        #[cfg(feature = "capture")]
        self.capture(ctx);
    }
}

#[cfg(feature = "capture")]
impl Wizard {
    fn capture(&mut self, ctx: &egui::Context) {
        let Some(path) = std::env::var_os("CAMEL_SETUP_CAPTURE_TO") else {
            return;
        };
        self.frames += 1;
        if let Some(pixels) = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        }) {
            let bytes: Vec<u8> = pixels.pixels.iter().flat_map(|p| p.to_array()).collect();
            if image::save_buffer(
                PathBuf::from(path),
                &bytes,
                pixels.size[0] as u32,
                pixels.size[1] as u32,
                image::ColorType::Rgba8,
            )
            .is_ok()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        } else if self.frames == 12 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        ctx.request_repaint_after(Duration::from_millis(60));
    }
}
