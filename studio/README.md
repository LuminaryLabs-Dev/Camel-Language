# Camel Studio — 0.2.0 preview

A native Rust, script-first IDE for Camel agent orchestration. Built with [eframe/egui](https://docs.rs/eframe/0.33.3/eframe/) and native [rfd file dialogs](https://docs.rs/rfd/0.15.4/rfd/). No browser, webview, JavaScript, or Python runtime is required.

![Actual native Camel Studio rendering](../docs/images/camel-studio.png)

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

## Workspace

- **Project / Outline:** `.camel` scripts in the opened folder and declarations from the actual parser. Build caches, Git directories, symlinks, and historical `dist` copies are excluded. Scanning is limited to 500 files and 12 directory levels.
- **Editor:** document tabs, dirty indicators, syntax coloring, line numbers, undo/redo and clipboard keyboard shortcuts, find, and source navigation. Files are UTF-8, up to 1 MiB. Windows uses Segoe UI / Consolas when installed, with bundled font fallbacks.
- **Orchestration:** choose an actual workflow and enter its typed parameters. Optional parameters can use source defaults. Agents, models, response contracts, tools, and policies are read from the AST, not invented or evaluated by the IDE.
- **Runs:** live workflow, agent, model-request, tool, and microphone events with status and elapsed time. Console contains script logs and worker diagnostics; Result contains the returned JSON; Problems links to source locations. Output belongs to the labeled last run, even if another document is selected.

The first launch opens `Camel/examples/orchestration.camel`: two bounded reviewers run concurrently and join their typed results. Nothing executes or records automatically.

## First offline run

Open `Camel/examples/programming_core.camel`, leave `count` at its source default of 6, and press **F5**. Result should contain Fibonacci **8** and sum-of-squares **91**. No model server is required.

For agent examples, start LM Studio at `http://localhost:1234` and load the model named in the script. The endpoint field configures the next run; it is not a claim that a server is connected. `CAMEL_LM_TOKEN`, `LM_API_TOKEN`, model-specific environment variables, and `CAMEL_LM_TIMEOUT_SECONDS` are inherited by the worker. Studio does not display or store tokens.

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

Saves stage content in the destination directory and replace the file after flush/sync. An external edit detected against the last saved snapshot is refused; use a new Save As filename to preserve both versions. Save As does not silently overwrite existing files. This check is not a cross-process lock: another writer racing the final replacement is outside its guarantee. Closing dirty tabs or quitting prompts to save, discard, or cancel; there is no crash recovery or session persistence yet.

Exactly one hidden worker subprocess is owned by Studio. Repeated Run while active is ignored. Stop and application exit terminate/reap only that worker. They do **not** undo completed writes, unload models, terminate LM Studio, or cancel an inference request already accepted by the server. Program-level cancellation remains a language roadmap item. Workflows are trusted code, not an OS sandbox.

Console retention is 2,000 lines (each displayed line capped at 16,384 characters), timeline retention is 2,000 operations, and individual worker protocol lines are limited to 1 MiB. This preview has no debugger, breakpoints, LSP completion, visual graph editor, extension marketplace, or model management UI.

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

This creates a new `Camel/dist/Camel-0.2.0-preview-studio.zip`, preserving older archives. Linux source builds need ALSA, Wayland/XKB development packages and a graphical session; Windows is the locally exercised platform.

Developer-only native render capture:

```powershell
cargo build --release --manifest-path .\studio\Cargo.toml --locked --features capture
$env:CAMEL_STUDIO_CAPTURE_TO = "$PWD\studio\target\studio.png"
& .\studio\target\release\camel-studio.exe
Remove-Item Env:CAMEL_STUDIO_CAPTURE_TO
```

The capture build waits for source checking and saves the actual OpenGL-rendered client area, then exits. Setting `CAMEL_STUDIO_CAPTURE_RUN=1` explicitly opts into running the opened workflow before capture; use only the offline example for render QA. Both environment hooks are absent from normal builds. Screenshot evidence is distinct from a manual interactive usability pass. Test coverage includes disk conflicts, repeat-safe saves, import-aware buffer checks, telemetry, real subprocess results/errors/stopping, and headless egui layout.
