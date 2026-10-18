# Camel Studio — v.0.3.0

A native Rust code editor and workflow runner for Camel agent orchestration. Edit Camel scripts directly, validate them, configure workflow inputs, and inspect execution results. Shipped examples and standard-library sources are kept out of the default project browser. Built with [eframe/egui](https://docs.rs/eframe/0.33.3/eframe/) and native [rfd file dialogs](https://docs.rs/rfd/0.15.4/rfd/). No browser, webview, JavaScript, or Python runtime is required.

## Launch

From the canonical `Camel-Language` repository, with current stable Rust:

```powershell
cargo run --release --manifest-path .\studio\Cargo.toml --locked
# Open a particular script:
cargo run --release --manifest-path .\studio\Cargo.toml --locked -- .\Camel\examples\audio_note_summarizer.camel
```

On this workstation the installed MSVC linker is damaged. Select LLVM before building:

```powershell
$env:RUSTFLAGS = "-C linker=lld-link"
```

After building, launch `studio\target\release\camel-studio.exe` directly. In a distribution ZIP, launch `bin\camel-studio.exe`; neither Cargo nor a separate Camel executable is needed to run Studio. Keep the extracted folder structure for its examples and documentation.

When the CLI and Studio are installed together, run `camel --gui` to open Studio. To select another Studio build, set `CAMEL_STUDIO_PATH` to its executable before running the command.

## Editor and project

- **Editor:** the central pane edits Camel source, with tabs, find, outline, and diagnostics. New launches open a fresh starter document; pass a script path on the command line to open a specific file.
- **Project:** the left browser lists user scripts. When running from a full Camel distribution, shipped examples and standard-library files are excluded from that list; use Open to choose a file explicitly. Build caches, Git directories, symlinks, and historical `dist` copies are excluded. Scanning is limited to 500 files and 12 directory levels.
- **Setup:** select a workflow and enter its inputs. Agents, models, integrations, data types, and safety rules appear in the inspector.
- **Runs:** live workflow, agent, model-request, tool, and microphone events with status and elapsed time. Console contains workflow logs and worker status; Result contains the returned JSON; Problems groups syntax, analysis, type, lint, and runtime diagnostics with source navigation.
- **Checkpoint and replay:** the per-run approval dialog accepts a checkpoint file, optional resume mode, persisted cassette record/replay, and an evaluation report destination. Replays are bound to the analyzed source and skip captured effects; checkpoint state and cassette files remain local run artifacts.
- **Library Manager:** open **Tools → Manage Libraries…** to search the catalog, inspect versions, and install, update, or remove libraries. Catalog refresh and package downloads run in the background. Packages install under `CAMEL_HOME/libraries`; the default is `%LOCALAPPDATA%/Camel/libraries` on Windows and `~/.camel/libraries` elsewhere.
- **Model Manager:** click **Models** in the main toolbar or open **Tools → Manage Models…** to search Hugging Face for GGUF repositories, inspect files, download one selected GGUF into a temporary local file, and register it in a running Ollama server. Select several installed models and load them together, choose context size per model, or unload them individually. Optional Hugging Face tokens are kept in memory only. Use **Use Ollama for workflow runs** to set the model service endpoint. In **Workflow setup → Agent models**, choose a model separately for every declared agent; the list grows with the source file and includes installed Ollama models, while the custom model ID field accepts IDs from other compatible endpoints. Blank overrides use the model configured in Camel source. These per-agent overrides last for the current Studio session and do not modify the `.camel` file.

The first launch does not open a packaged example or run anything automatically.

## First offline run

Open `Camel/examples/programming_core.camel`, leave `count` at its recommended value of 6, and press **F5**. Result should contain Fibonacci **8** and sum-of-squares **91**. No model server is required.

For agent examples, start LM Studio at `http://localhost:1234` and load the model named in the script. The endpoint field configures the next run; it is not a claim that a server is connected. `CAMEL_LM_TOKEN`, `LM_API_TOKEN`, model-specific environment variables, and `CAMEL_LM_TIMEOUT_SECONDS` are inherited by the worker. Studio does not display or store tokens.

For a parallel multi-model workflow, start Ollama at `http://localhost:11434`, import and load the chat-capable models in Model Manager, and open `Camel/examples/multi_model_review.camel`. In **Agent models**, select a model for each agent or keep the source configuration. Each agent keeps its own model and temperature; `spawn` starts model requests before the workflow awaits their results. Ollama may queue or unload models when available memory is insufficient.

For microphone notes, open `Camel/examples/audio_note_summarizer.camel`. `Main` records from the microphone; `FromFile` accepts existing audio. **Finish recording** sends Enter to a `record(0)` session; timed recording ends at the script's duration. **Stop** aborts the entire worker rather than returning partial audio. Liquid/LM Studio audio inference still depends on a server accepting `input_audio`; successful inference with those models has not been established here.

## Shortcuts and safety

| Action | Shortcut |
| --- | --- |
| New / Open / Save | Ctrl+N / Ctrl+O / Ctrl+S |
| Save As | Ctrl+Shift+S |
| Find / dismiss find | Ctrl+F / Escape |
| Check | Ctrl+F5 |
| Run workflow | F5 |
| Stop | Shift+F5 |
| Undo / Redo | Ctrl+Z / Ctrl+Shift+Z |

Run saves a dirty entry file first (new files prompt for a destination). Save other edited scripts before running: imported modules are loaded from disk, not composed from unsaved tabs. Check inspects the current buffer and resolves its imports from disk, without model calls, configuration evaluation, tool effects, or microphone access. Checks are debounced, performed off the UI thread, and stale results are discarded.

Problems groups syntax, analysis, type, lint, and runtime findings. It shows full messages and codes; selecting a source diagnostic opens the corresponding file and moves the editor to the reported token range. Error ranges are highlighted. Lint warnings are informational and do not block Run.

Saves stage content in the destination directory and replace the file after flush/sync. An external edit detected against the last saved snapshot is refused; use a new Save As filename to preserve both versions. Save As does not silently overwrite existing files. This check is not a cross-process lock: another writer racing the final replacement is outside its guarantee. Closing dirty tabs or quitting prompts to save, discard, or cancel; there is no crash recovery or session persistence yet.

Exactly one hidden worker subprocess is owned by Studio. Repeated Run while active is ignored. Stop and application exit terminate/reap only that worker. They do **not** undo completed writes, unload models, terminate LM Studio, or cancel an inference request already accepted by the server. Program-level cancellation remains a language roadmap item. Workflows are trusted code, not an OS sandbox.

Console retention is 2,000 lines (each displayed line capped at 16,384 characters), timeline retention is 2,000 operations, and individual worker protocol lines are limited to 1 MiB. Hugging Face imports currently accept one GGUF file at a time; importing arbitrary Safetensors checkpoints, split GGUF sets, and model deletion are not included. Ollama's server controls how many concurrent requests fit in available memory. This preview has no debugger, breakpoints, LSP completion, visual graph editor, or extension marketplace.

## Verify and package

```powershell
cargo fmt --manifest-path .\studio\Cargo.toml --check
cargo clippy --manifest-path .\studio\Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path .\studio\Cargo.toml --locked
cargo test --manifest-path .\runtime\Cargo.toml --locked
cargo build --release --manifest-path .\runtime\Cargo.toml --locked
cargo build --release --manifest-path .\studio\Cargo.toml --locked
pwsh .\scripts\package-preview.ps1 -IncludeStudio
```

This creates a new `Camel/dist/Camel-v.0.3.0-windows-x86_64-studio-setup.zip`, preserving older archives. The current v.0.3.0 distribution target is Windows.

Developer-only native render capture:

```powershell
cargo build --release --manifest-path .\studio\Cargo.toml --locked --features capture
$env:CAMEL_STUDIO_CAPTURE_TO = "$PWD\studio\target\studio.png"
& .\studio\target\release\camel-studio.exe
Remove-Item Env:CAMEL_STUDIO_CAPTURE_TO
```

The capture build waits for source checking and saves the actual OpenGL-rendered client area, then exits. Setting `CAMEL_STUDIO_CAPTURE_RUN=1` explicitly opts into running the opened workflow before capture; use only the offline example for render QA. Both environment hooks are absent from normal builds. Screenshot evidence is distinct from a manual interactive usability pass. Test coverage includes disk conflicts, repeat-safe saves, import-aware buffer checks, telemetry, real subprocess results/errors/stopping, and headless egui layout.
