#![cfg_attr(windows, windows_subsystem = "windows")]
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut bundle = std::env::current_exe()?
        .parent()
        .ok_or("Missing executable parent.")?
        .to_path_buf();
    let mut install_to = None;
    let mut add_to_path = true;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--bundle" | "--install-to" => {
                let flag = &args[i];
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| format!("{flag} requires a folder."))?;
                if flag == "--bundle" {
                    bundle = value.into();
                } else {
                    install_to = Some(value.into());
                }
            }
            "--no-path" => add_to_path = false,
            other => return Err(format!("Unknown setup option: {other}").into()),
        }
        i += 1;
    }
    if let Some(destination) = install_to {
        let bundle = camel_setup::bundle::Bundle::load(&bundle)?;
        let request = camel_setup::install::Request {
            destination,
            add_to_path,
        };
        let receipt = camel_setup::install::install(&bundle, &request, |step| eprintln!("{step}"))?;
        println!("{}", serde_json::to_string_pretty(&receipt)?);
        return Ok(());
    }
    eframe::run_native(
        "Camel Setup",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([860.0, 740.0])
                .with_min_inner_size([780.0, 700.0]),
            renderer: eframe::Renderer::Glow,
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(ui::Wizard::new(cc, bundle)))),
    )
    .map_err(|e| e.to_string().into())
}
