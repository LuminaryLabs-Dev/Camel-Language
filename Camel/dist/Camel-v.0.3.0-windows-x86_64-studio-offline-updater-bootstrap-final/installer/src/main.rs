#![cfg_attr(windows, windows_subsystem = "windows")]
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut bundle = std::env::current_exe()?
        .parent()
        .ok_or("Missing executable parent.")?
        .to_path_buf();
    let mut install_to = None;
    let mut destination = None;
    let mut wait_for_pid = None;
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
            "--destination" | "--wait-for-pid" => {
                let flag = &args[i];
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| format!("{flag} requires a value."))?;
                if flag == "--destination" {
                    destination = Some(value.into());
                } else {
                    wait_for_pid = Some(value.parse::<u32>().map_err(|_| "Invalid process id.")?);
                }
            }
            "--no-path" => add_to_path = false,
            other => return Err(format!("Unknown setup option: {other}").into()),
        }
        i += 1;
    }
    if let Some(pid) = wait_for_pid {
        wait_for_process(pid)?;
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
        Box::new(move |cc| Ok(Box::new(ui::Wizard::new(cc, bundle, destination)))),
    )
    .map_err(|e| e.to_string().into())
}

fn wait_for_process(pid: u32) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let status = std::process::Command::new("powershell.exe")
            .creation_flags(0x08000000)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Wait-Process -Id $env:CAMEL_SETUP_WAIT_PID -ErrorAction SilentlyContinue",
            ])
            .env("CAMEL_SETUP_WAIT_PID", pid.to_string())
            .status()?;
        if !status.success() {
            return Err("Could not wait for the Camel command to exit.".into());
        }
    }
    #[cfg(unix)]
    {
        while std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()?
            .success()
        {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    Ok(())
}
