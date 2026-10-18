#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--worker") {
        std::process::exit(camel_studio::worker::main(&args[1..]));
    }
    let initial = args.first().map(std::path::PathBuf::from);
    eframe::run_native(
        "Camel Studio",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([1320.0, 880.0])
                .with_min_inner_size([920.0, 640.0]),
            renderer: eframe::Renderer::Glow,
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(camel_studio::app::Studio::new(cc, initial)))),
    )
}
