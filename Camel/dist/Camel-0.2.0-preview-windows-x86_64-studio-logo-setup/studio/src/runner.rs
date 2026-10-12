//! Owns exactly one child. Pipe readers never touch UI state; Drop always reaps it.
use crate::worker::Message;
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
};

#[derive(Debug)]
pub enum Update {
    Message(Message),
    Console(String),
    Closed,
}
pub struct Runner {
    child: Child,
    rx: Receiver<Update>,
    readers: usize,
    exit: Option<std::process::ExitStatus>,
    pub stopped: bool,
}
impl Runner {
    pub fn start(
        executable: &Path,
        source: &Path,
        entry: &str,
        args: &[String],
        endpoint: &str,
    ) -> Result<Self, String> {
        let mut command = Command::new(executable);
        command
            .arg("--worker")
            .arg(source)
            .arg(entry)
            .arg(serde_json::to_string(args).map_err(|e| e.to_string())?)
            .env("CAMEL_LM_ENDPOINT", endpoint)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (tx, rx) = mpsc::sync_channel(256);
        let first = tx.clone();
        if let Err(e) = thread::Builder::new()
            .name("studio-events".into())
            .spawn(move || read(stdout, first, true))
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e.to_string());
        }
        if let Err(e) = thread::Builder::new()
            .name("studio-stderr".into())
            .spawn(move || read(stderr, tx, false))
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e.to_string());
        }
        Ok(Self {
            child,
            rx,
            readers: 2,
            exit: None,
            stopped: false,
        })
    }
    pub fn poll(&mut self) -> Result<(Vec<Update>, Option<bool>), String> {
        let mut updates = Vec::new();
        for _ in 0..512 {
            match self.rx.try_recv() {
                Ok(Update::Closed) => self.readers = self.readers.saturating_sub(1),
                Ok(u) => updates.push(u),
                Err(_) => break,
            }
        }
        if self.exit.is_none() {
            self.exit = self.child.try_wait().map_err(|e| e.to_string())?;
        }
        let done = self
            .exit
            .filter(|_| self.readers == 0)
            .map(|s| s.success() && !self.stopped);
        Ok((updates, done))
    }
    pub fn finish_recording(&mut self) -> Result<(), String> {
        self.child
            .stdin
            .as_mut()
            .ok_or("Recording input is unavailable")?
            .write_all(b"\n")
            .map_err(|e| e.to_string())
    }
    pub fn stop(&mut self) -> Result<(), String> {
        if self.stopped || self.exit.is_some() {
            return Ok(());
        }
        if self.child.try_wait().map_err(|e| e.to_string())?.is_none() {
            self.child.kill().map_err(|e| e.to_string())?;
        }
        self.stopped = true;
        Ok(())
    }
}
impl Drop for Runner {
    fn drop(&mut self) {
        // Disconnect bounded senders first, so reader threads cannot block during teardown.
        let (_, empty) = mpsc::sync_channel(1);
        self.rx = empty;
        if self.exit.is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
fn read(stream: impl Read, tx: SyncSender<Update>, protocol: bool) {
    let mut input = BufReader::new(stream);
    loop {
        let mut line = Vec::new();
        let count = match input.by_ref().take(1_048_577).read_until(b'\n', &mut line) {
            Ok(n) => n,
            Err(e) => {
                let _ = tx.send(Update::Console(format!("Pipe error: {e}")));
                break;
            }
        };
        if count == 0 {
            break;
        }
        if line.len() > 1_048_576 {
            let _ = tx.send(Update::Console(
                "Worker line exceeds 1 MiB; output rejected".into(),
            ));
            break;
        }
        let text = String::from_utf8_lossy(&line).trim_end().to_owned();
        let update = if protocol {
            match serde_json::from_str(&text) {
                Ok(message) => Update::Message(message),
                Err(_) => Update::Console(format!(
                    "Invalid worker output: {}",
                    text.chars().take(400).collect::<String>()
                )),
            }
        } else {
            Update::Console(text)
        };
        if tx.send(update).is_err() {
            return;
        }
    }
    let _ = tx.send(Update::Closed);
}
