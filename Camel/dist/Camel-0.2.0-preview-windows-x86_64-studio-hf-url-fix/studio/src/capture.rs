//! Explicit opt-in QA of the real native renderer, absent from ordinary builds.
use eframe::egui;
use std::path::PathBuf;

pub struct Capture {
    path: Option<PathBuf>,
    frames: usize,
    requested: bool,
    pub run_pending: bool,
}
impl Default for Capture {
    fn default() -> Self {
        let path = std::env::var_os("CAMEL_STUDIO_CAPTURE_TO").map(PathBuf::from);
        let run_pending =
            path.is_some() && std::env::var("CAMEL_STUDIO_CAPTURE_RUN").is_ok_and(|s| s == "1");
        Self {
            path,
            frames: 0,
            requested: false,
            run_pending,
        }
    }
}
impl Capture {
    pub fn active(&self) -> bool {
        self.path.is_some()
    }
    /// Returns true after a screenshot is written, so the app can permit its own close.
    pub fn tick(&mut self, ctx: &egui::Context, settled: bool) -> bool {
        let Some(path) = &self.path else {
            return false;
        };
        self.frames += 1;
        let screenshot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(pixels) = screenshot {
            let bytes: Vec<u8> = pixels.pixels.iter().flat_map(|p| p.to_array()).collect();
            image::save_buffer(
                path,
                &bytes,
                pixels.size[0] as u32,
                pixels.size[1] as u32,
                image::ColorType::Rgba8,
            )
            .expect("native capture must be writable");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return true;
        }
        if settled && self.frames >= 10 && !self.requested {
            self.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
        false
    }
}
