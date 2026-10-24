#![cfg_attr(windows, windows_subsystem = "windows")]
mod ui;

fn main() {
    if let Err(error) = run() {
        let message = format!("Camel Setup could not start.\n\n{error}");
        #[cfg(windows)]
        show_startup_error(&message);
        eprintln!("{message}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

#[cfg(windows)]
fn show_startup_error(message: &str) {
    use std::os::windows::ffi::OsStrExt;
    use std::{ffi::OsStr, ptr};

    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(
            window: *mut core::ffi::c_void,
            text: *const u16,
            caption: *const u16,
            kind: u32,
        ) -> i32;
    }

    let text: Vec<u16> = OsStr::new(message).encode_wide().chain(Some(0)).collect();
    let caption: Vec<u16> = OsStr::new("Camel Setup")
        .encode_wide()
        .chain(Some(0))
        .collect();
    unsafe {
        MessageBoxW(ptr::null_mut(), text.as_ptr(), caption.as_ptr(), 0x10);
    }
}

fn wait_for_process(pid: u32) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    #[cfg(windows)]
    {
        use std::ffi::c_void;

        #[link(name = "kernel32")]
        extern "system" {
            fn OpenProcess(access: u32, inherit_handle: i32, process_id: u32) -> *mut c_void;
            fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
            fn CloseHandle(handle: *mut c_void) -> i32;
            fn GetLastError() -> u32;
        }

        const SYNCHRONIZE: u32 = 0x0010_0000;
        const INFINITE: u32 = u32::MAX;
        const WAIT_OBJECT_0: u32 = 0;
        const WAIT_FAILED: u32 = u32::MAX;
        const ERROR_INVALID_PARAMETER: u32 = 87;

        let handle = unsafe { OpenProcess(SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            let error = unsafe { GetLastError() };
            if error == ERROR_INVALID_PARAMETER {
                return Ok(());
            }
            return Err(format!(
                "Could not open Camel process {pid} for waiting (Windows error {error})."
            )
            .into());
        }

        let result = unsafe { WaitForSingleObject(handle, INFINITE) };
        let error = if result == WAIT_FAILED {
            Some(unsafe { GetLastError() })
        } else {
            None
        };
        unsafe { CloseHandle(handle) };
        if result != WAIT_OBJECT_0 {
            return Err(format!(
                "Waiting for Camel process {pid} failed (Windows error {}).",
                error.unwrap_or(result)
            )
            .into());
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
