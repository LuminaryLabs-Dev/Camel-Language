# Camel Project Memory

## Intent

The user is creating Camel, a programming language for agentic programming, and asked for the repository to be restored as thoroughly as possible, including `.gitignore`.

## Findings — 2026-09-04

- The repository root is `C:\Github\Camel\Camel-Language`.
- The configured remote is `https://github.com/LuminaryLabs-Dev/Camel-Language.git`.
- Local and remote Git history contain only the initial commit.
- The initial commit contains `.gitattributes`, `LICENSE`, and `README.md`; the deleted working-tree copies were restored.
- No source, build configuration, tests, branches, tags, reflog entries, or unreachable objects contain the compiler/runtime implementation.
- The available `Camel` directory contains documentation and a compiled Windows demo.
- The compiled `camel.exe` reports version `0.1.0-alpha` and successfully checks `encyclopedia.camel`.
- A repository `.gitignore` now covers Rust builds, generated runtime output, binaries, packages, secrets, editor files, and temporary artifacts while retaining demo source/instructions.
- Recovery provenance is documented in `docs/REPOSITORY_RECOVERY.md`.

## Recommendation

Treat the compiled demo and documentation as recovered reference material. Before extending Camel, locate the original Rust source or formally restart implementation with a written specification and a tested parser/runtime/end-to-end vertical slice.

## New intent — 2026-10-09

The user requested a Camel script that sends audio to `LiquidAI/LFM2.5-Audio-1.5B-GGUF` for transcription, then sends the transcript to `LiquidAI/LFM2.5-1.2B-Thinking` for a topic-prioritized note summary, with command-line run instructions.

## New findings

- The existing Camel syntax supports object prompts and local image paths, but repository documentation does not establish an audio input field.
- Current LM Studio public REST/OpenAI-compatible documentation establishes text and image input, not audio input.
- Liquid AI's model card establishes that `LFM2.5-Audio-1.5B-GGUF` supports ASR, but the serving path must expose audio to use it.
- Added `Camel/examples/audio_note_summarizer.camel` with bounded, local-only transcription and summarization agents.
- Added `Camel/docs/AUDIO_NOTE_SUMMARIZER.md` with PowerShell commands, model setup, JSON output, and the audio-support limitation.
- The bundled validator initially found an invalid return from the void workflow; the script now emits `json.stringify(summaryResult.data)` through `SUMMARY_JSON` instead.
- Run instructions were aligned with the bundled CLI: its help output has no `--lm` switch because LM Studio routing is implicit; a newer build may expose that flag.
- The missing general runtime was not recoverable, so a focused source-backed Rust crate was added at `runtime/` for the audio note workflow.
- The runtime resolves relative audio paths, validates extensions, base64-encodes bytes, sends OpenAI-compatible `input_audio`, performs bounded sequential transcription and summarization calls, and returns typed JSON.
- `cargo test --manifest-path runtime/Cargo.toml` passes five unit tests when using the available LLVM linker; LM Studio itself was not reachable for live inference validation.
- Relative audio paths now prefer the caller's working directory and fall back to the `.camel` source directory, matching the documented CLI examples.
- The LM Studio endpoint normalizer accepts either the repository-style root URL or the OpenAI-compatible `/v1` URL without duplicating the path.
- The first release build hit a rustc ICE inside unused TLS dependency `ring`; the focused local runtime now omits TLS support to keep its dependency boundary small and local-HTTP-specific.
- The release executable now builds successfully with the available LLVM linker, validates the workflow, and exits successfully for `--help`.
- Packaged the source-only runtime bundle at `Camel/dist/Camel-Audio-Runtime.zip` with preserved `Camel-Audio-Runtime/runtime` and `Camel-Audio-Runtime/Camel` directory structure; build output was excluded.
- Repacked the ZIP with `runtime/` and `Camel/` at its archive root after the first package caused an extra nested `Camel-Audio-Runtime` directory during extraction.
- The audio workflow is now microphone-first: `record` captures from the default input device until a bounded duration or Enter, encodes WAV in memory, optionally saves it, then performs the existing two-agent LM Studio calls.
- Microphone smoke test successfully captured for one second; the run then failed only at the expected LM Studio HTTP request because no server was listening on localhost:1234.
- The user's prior extracted archive remains under `Camel/dist/Camel-Audio-Runtime/Camel-Audio-Runtime`; it was left untouched, and the refreshed ZIP has a flat archive root.

## New intent — 2026-10-10

The user asked what Camel is missing to qualify as a fully new language.

The user clarified that Camel's intended goal is a general-purpose programming language for programming agents, not merely an agent workflow DSL. Future design recommendations should preserve a general programming core while making agents, tools, models, state, and policies first-class language capabilities.

## Language maturity assessment

- The checked-in source does not contain the general Camel lexer, parser, AST, semantic analyzer, interpreter/compiler, or ToolRegistry described by the architecture article.
- The source-backed Rust crate is a specialized audio demo host: it checks that certain expected text fragments occur in a Camel source file, then runs a hard-coded capture/transcription/summary sequence; it does not parse or execute general Camel programs.
- The microphone `record(durationSeconds)` construct shown in the Camel example is not implemented as a general Camel builtin; microphone capture is currently selected through a runtime-specific CLI subcommand.
- To become a language project rather than a demo, Camel needs a recoverable source of truth, normative grammar and semantics, a general compiler/interpreter, and conformance tests before breadth features.

## Implementation intent — 2026-10-10

The user approved implementing the general-purpose agent language foundation. Existing recovered files, the alpha binary, the previous audio Rust API, and the user's extra-nested extracted audio archive must remain intact.

## Implemented foundation — 0.2.0 preview

- Replaced the audio CLI's source-fragment checks and hard-coded orchestration with an actual lexer, source-located AST, parser, import loader, declaration/name checker, value model and interpreter under runtime/src/language.
- Added ordinary typed functions, recursion, first-class named function/agent/tool references, default parameters, lexical scopes, arrays/records, source-order initializers, if/while/for/foreach, checked arithmetic and try/catch/finally.
- Added actual spawn/await tasks with snapshot local state, cached awaits, observed-error handling, total-task/call-depth/shared-step limits, and draining before execution returns.
- Added injected ModelProvider and ToolRegistry interfaces. Agent names/models/system prompts/tool lists/response records now derive from source, not fixed demo assumptions.
- Added strict model record validation, schema instructions, bounded provider retries/turns, tool-result conversation history, pre-dispatch capability policy checks, typed tool arguments and invocation-wide duplicate-call-ID rejection.
- Added staged root assignments and synced same-directory atomic file replacement. Host effect batches are not transactions; trusted source/host execution is not an OS sandbox.
- Converted record(seconds) into an actual language builtin. The audio source now declares Main for microphone capture and FromFile for explicit diagnostic input, sharing a source-defined Summarize function.
- Kept the existing Rust audio helper APIs for compatibility; their old lenient summary helper is not used by typed Camel agent response execution.
- Made camel the default executable and retained camel-audio-runtime as an implementation-sharing CLI alias. Check now requires no audio/entry arguments and never records or calls models.
- Added ordinary programming and typed-agent examples, the normative preview specification, honest runtime/audio documentation, and Windows/Linux CI configuration. Linux CI is configured but has not been executed locally.
- No recommended plugin is required: direct Rust implementation and the installed local toolchain provide the necessary compiler/runtime surfaces. No plugin was installed.
- Used camel-agent-programming for executable-source semantics and atomize-it for ownership, bounded execution and staged state/file commits.

## Verification and remaining boundaries

- Windows offline tests pass: 47 tests across audio helpers, CLI aliases, CLI integration, local-loopback HTTP provider integration and language conformance.
- cargo clippy --all-targets -- -D warnings passes; release executables build with the installed LLVM linker via RUSTFLAGS=-C linker=lld-link.
- Release CLI validates both agent examples without inference and runs programming_core.camel with count=6, returning Fibonacci 8 and sum-of-squares 91.
- The HTTP integration test uses a local fake server, proving request routing, /v1 normalization, authorization and typed result extraction. It does not establish real model inference.
- Automated tests inject microphone data; no microphone is opened during this implementation verification. Live LM Studio/Liquid audio inference remains unverified; input_audio serving and any model-specific resampling remain provider integration work.
- Preview types are checked at runtime boundaries; check is not full static inference or return-path proof. Var does not infer record schemas. Packages/namespaces/closures/native compilation/LSP/cancellation/durable agent state/streamed transcription/HTTPS are not implemented.
