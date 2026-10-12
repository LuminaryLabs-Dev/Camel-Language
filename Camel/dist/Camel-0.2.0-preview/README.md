# Camel

Camel is a general-purpose language for programming agents. The source-backed **0.2.0 preview** executes ordinary programs as well as typed agent/tool workflows.

The recovered alpha executable remains reference material; `runtime/` is the current implementation. This is a new interpreter foundation, not a recovered production compiler or a complete 1.0 language.

## Start here

From the repository root (`Camel-Language`), with Rust 1.88 or newer:

```powershell
cargo test --manifest-path .\runtime\Cargo.toml --locked
cargo run --manifest-path .\runtime\Cargo.toml -- check .\Camel\examples\programming_core.camel
cargo run --manifest-path .\runtime\Cargo.toml -- .\Camel\examples\programming_core.camel --json -- 6
```

The last command runs without a model server and returns Fibonacci 8 and sum-of-squares 91. On this workstation, if the installed MSVC linker fails, use the installed LLVM linker first:

```powershell
$env:RUSTFLAGS = "-C linker=lld-link"
```

## Implementation

- Lexer, source-located AST, parser, import loader, name/declaration checks, and general interpreter.
- Functions, recursion, named function references, lexical scope, records, arrays, control flow, exceptions, and bounded concurrent tasks.
- First-class agent and tool references, provider injection, typed model responses, capability-gated tool execution, and turn/retry budgets.
- Ordinary file tools with staged atomic replacement; microphone recording and audio loading as actual language builtins.
- Offline conformance tests and Windows/Linux CI configuration. Linux builds require ALSA development headers.

See the [preview language specification](docs/LANGUAGE_SPEC.md), [runtime guide](runtime/README.md), [audio workflow guide](Camel/docs/AUDIO_NOTE_SUMMARIZER.md), and [repository recovery history](docs/REPOSITORY_RECOVERY.md).

Examples: [ordinary programming](Camel/examples/programming_core.camel), [agent planning](Camel/examples/agent_planner.camel), and [microphone notes](Camel/examples/audio_note_summarizer.camel).

To create a Windows preview ZIP after building release binaries, run `pwsh ./scripts/package-preview.ps1`. It packages source, tests, documentation and `bin/camel.exe` with a flat archive root and SHA-256 manifest, excluding build caches. Existing archives are preserved; choose `-Output` for a new destination.

## Honest boundaries

Type contracts are enforced at runtime; `check` does not yet perform whole-program static type inference or prove all return paths. No package manager, namespaces, closures, user generics, native compilation, debugger/LSP, persistent agent memory, streaming audio, or task cancellation is implemented.

Agent policies govern model-requested tools, not trusted workflow code or the configured inference endpoint. This is not an operating-system sandbox. The shipped HTTP adapter does not include HTTPS support.

Audio payload construction is tested with a fake provider. **Successful LM Studio/Liquid audio inference is not verified**; a compatible server must accept `input_audio` for the requested model.

Licensed under Apache-2.0; see LICENSE.
