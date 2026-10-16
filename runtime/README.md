# Camel source runtime — 0.2.0 preview

This crate parses and executes the versioned Camel 0.2 language with initial static expression checks. Agent names, models, prompts, workflow arguments, control flow, records and tool calls come from the program. The audio inference example's agents and prompts are in the separately versioned `libraries/audio` source package; the runtime retains generic audio host input support.

The package/library name `camel-audio-runtime` / `camel_audio_runtime` remains for compatibility. `camel` is the default executable; `camel-audio-runtime` is an alias using the same implementation. Existing Rust audio helper APIs remain available, including their legacy lenient summarization helper; Camel's `response = Record` path is strict.

## Build and verify

From the repository root:

```powershell
cargo fmt --manifest-path .\runtime\Cargo.toml --check
cargo test --manifest-path .\runtime\Cargo.toml --locked
cargo clippy --manifest-path .\runtime\Cargo.toml --locked --all-targets -- -D warnings
cargo build --release --manifest-path .\runtime\Cargo.toml --locked
& .\runtime\target\release\camel.exe check .\Camel\examples\programming_core.camel
& .\runtime\target\release\camel.exe .\Camel\examples\programming_core.camel --json -- 6
```

Use Rust 1.88+. If the local MSVC installation cannot link, set `$env:RUSTFLAGS = "-C linker=lld-link"` when LLVM is installed and on PATH. This is a local workaround, not required on a healthy MSVC setup.

On Linux, install `libasound2-dev` and `pkg-config` before building CPAL. Linux CI is configured but has not been run locally.

## CLI

```text
camel check file.camel
camel [run] file.camel [--workflow Main] [--json] [--trace] [--steps N] -- [arguments]
camel record file.camel [--seconds N|--until-enter] [--save-recording path] -- [arguments]
camel --gui [file.camel]
camel library [list|search <query>|install <name>|update <name>|remove <name>]
```

`camel --gui` launches Camel Studio. The CLI searches for `camel-studio` beside itself, then in this repository's Studio build output using the CLI's matching debug/release profile first, then on `PATH`. Set `CAMEL_STUDIO_PATH` to select a specific executable. An optional `.camel` path is passed through to Studio.

The source catalog is `libraries/index.json`. It declares each library's independent version and Git revision. The CLI reads this checkout's catalog for development and uses the remote GitHub catalog for updates after packaging; remote installs require access to both the catalog and source repository. Use `camel library list` or `camel library search <query>` to discover packages, `install`/`update` to download a source snapshot, and `remove` to uninstall it. Set `CAMEL_HOME` to choose the installation root; otherwise libraries live under the user's local application data directory on Windows or `~/.camel` on Unix. Programs can import installed libraries with `using "@camel/standard/core.camel";`; local relative imports retain precedence.

Arguments after `--` bind to the chosen workflow's declared parameters. Defaults come from source. Strings are passed literally; numeric and boolean parameters are parsed; arrays and records accept JSON. `check` validates syntax, names, language version, and statically knowable expression types. It requires no workflow arguments and does not open microphones, call tools, or contact a model.

The runtime's `analyze_file`, `analyze_buffer`, and `analyze_source` APIs return an `AnalysisResult` with structured diagnostics: severity, phase, stable rule code, token source range, related locations, and optional help. Syntax analysis still stops at its first error; semantic and type checks collect independent errors across declarations and top-level statements. The warning-only linter reports unused locals, parameters, declarations, and imported declarations; warnings do not block execution. Studio groups Problems by phase, shows full messages, highlights ranges, and opens the source when a diagnostic is selected.

`record` is a compatibility command, not a special interpreter for one demo: it overrides durations of `record(seconds)` calls. It defaults to recording until Enter. Normal `run` uses the source's duration. `--lm` remains accepted, but is unnecessary.

`--json` prints one JSON object: `{"value": <workflow return>, "logs": [...]}`. Microphone prompts and trace messages go to stderr. Logs are buffered until successful workflow completion; they are not a streaming progress channel.

## Runtime ownership

- `language/syntax.rs`: source sites, AST, type declarations.
- `language/lexer.rs`, `parser.rs`, `check.rs`: syntax and declaration validation.
- `language/static_types.rs`: expression type inference and early mismatch diagnostics.
- `language/mod.rs`: source/import loading and compiled-program ownership.
- `language/eval.rs`, `value.rs`: execution, scopes, type boundaries, budget and task lifecycle.
- `language/agents.rs`: model protocol, typed outputs, policy decisions and tool loops.
- `language/host.rs`: registered handlers, provider/recorder injection and host capabilities.
- `lib.rs`: existing CPAL/WAV helpers and local HTTP model client.
- `main.rs`: CLI parsing and presentation, not workflow logic.
- `../packages/camel-library-manager`: shared library catalog, HTTPS installation, removal, and installed-source lookup. The runtime re-exports this API as `libraries`.

Each `Engine::run` owns fresh scopes, logs, tasks, and a shared step budget. Named functions receive values, not caller-local mutable references. Tasks snapshot local state; all tasks are drained before execution returns. Awaiting a completed task never repeats its work.

Assignments stage a copy and validate the entire declared root type before replacing the binding. File writes stage bytes in a same-directory temporary file, sync, then rename/replace. Tool batches are not cross-handler transactions; already committed effects are not undone if a later tool or model fails.

## Host extension points

Embed `CompiledProgram`, `Engine`, `Host`, and `Value` from `camel_audio_runtime::language`.

- Implement `ModelProvider::complete(request)` to return an assistant-message JSON object with `content` and optional `tool_calls`.
- Register a handler through `ToolRegistry::register(name, capability, handler)`. Duplicate registrations fail without replacing the previous handler.
- Inject a recorder through `Host.recorder` for alternate audio backends or deterministic tests.
- Configure `Limits` to bound steps, function depth and total spawned tasks.

The CLI uses the OpenAI-compatible local HTTP adapter. `CAMEL_LM_ENDPOINT` defaults to `http://localhost:1234`; root and `/v1` URLs normalize to one `/v1/chat/completions` path. Optional tokens: `CAMEL_LM_TOKEN`, falling back to `LM_API_TOKEN`. Timeout: `CAMEL_LM_TIMEOUT_SECONDS`, default 300.

Model IDs come entirely from the program, commonly via `env("CAMEL_LM_MODEL", "...")`. Audio examples also use `CAMEL_AUDIO_MODEL` and `CAMEL_SUMMARY_MODEL`; those names have no special interpreter meaning.

## Verification scope

Runtime tests use deterministic fake providers/recorders and temporary files. The versioned [`conformance/0.2`](../conformance/0.2/manifest.json) fixture suite defines accepted and rejected programs for the current language edition. Runtime tests also cover ordinary programming, agents, strict record outputs, policy denials before dispatch, tool-result history, retry budgets, microphone request construction, imports and safe writes.

No live microphone is opened by automated tests. No live LM Studio/Liquid inference is claimed. HTTP(S) transport is implemented; server-specific audio adaptation and streaming are not.

See [the language specification](../docs/LANGUAGE_SPEC.md) for precise supported syntax and limitations.
