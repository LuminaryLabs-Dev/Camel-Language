# Camel source runtime — 0.2.0 preview

This crate now parses and executes Camel source. Agent names, models, prompts, workflow arguments, control flow, records and tool calls come from the program; there is no hard-coded audio workflow in the CLI.

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
```

Arguments after `--` bind to the chosen workflow's declared parameters. Defaults come from source. Strings are passed literally; numeric and boolean parameters are parsed; arrays and records accept JSON. `check` requires no workflow arguments and does not open microphones, call tools, or contact a model.

`record` is a compatibility command, not a special interpreter for one demo: it overrides durations of `record(seconds)` calls. It defaults to recording until Enter. Normal `run` uses the source's duration. `--lm` remains accepted, but is unnecessary.

`--json` prints one JSON object: `{"value": <workflow return>, "logs": [...]}`. Microphone prompts and trace messages go to stderr. Logs are buffered until successful workflow completion; they are not a streaming progress channel.

## Runtime ownership

- `language/syntax.rs`: source sites, AST, type declarations.
- `language/lexer.rs`, `parser.rs`, `check.rs`: syntax and declaration validation.
- `language/mod.rs`: source/import loading and compiled-program ownership.
- `language/eval.rs`, `value.rs`: execution, scopes, type boundaries, budget and task lifecycle.
- `language/agents.rs`: model protocol, typed outputs, policy decisions and tool loops.
- `language/host.rs`: registered handlers, provider/recorder injection and host capabilities.
- `lib.rs`: existing CPAL/WAV helpers and local HTTP model client.
- `main.rs`: CLI parsing and presentation, not workflow logic.

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

Conformance tests use deterministic fake providers/recorders and temporary files. They cover ordinary programming, renamed/source-configured agents, strict record outputs, policy denials before dispatch, tool-result history, retry budgets, microphone request construction, imports and safe writes.

No live microphone is opened by automated tests. No live LM Studio/Liquid inference is claimed. HTTP is implemented; HTTPS, server-specific audio adaptation and streaming are not.

See [the language specification](../docs/LANGUAGE_SPEC.md) for precise supported syntax and limitations.
