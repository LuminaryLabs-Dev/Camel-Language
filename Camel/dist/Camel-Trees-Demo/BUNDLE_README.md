# Continuous Encyclopedia of Trees

This folder is a self-contained Camel demo for LM Studio.

## Run

1. Install and open LM Studio.
2. Download/load any chat-capable model.
3. Start LM Studio's local server on `http://localhost:1234`.
4. Run `run-bundle.ps1` in PowerShell (`powershell -ExecutionPolicy Bypass -File .\run-bundle.ps1` if scripts are restricted).
5. Enter the loaded model identifier when prompted.
6. Press `Ctrl+C` whenever you want to stop.

Each cycle creates a new HTML page under `encyclopedia/trees` and saves the full agent conversation under `logs`. The included `camel.exe` is the only executable dependency; no Rust, Cargo, or source checkout is required.
