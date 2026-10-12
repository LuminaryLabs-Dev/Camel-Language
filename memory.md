# Camel Project Memory

## High-level next-step planning — 2026-10-12

- User asks for a high-level sequence to organize Camel's next steps; this is planning only.
- Current project has a working Rust language/runtime, Camel Studio, library manager, and native cross-platform installer/distributions. Latest memory records successful Windows/macOS/Linux distribution CI and archive verification; visible native GUI QA was Windows-only.
- The canonical worktree still has pending installer, packaging, package-manager, and README changes. Treat release closure and a clean, reproducible cross-platform preview as the immediate milestone before expanding the language.
- Suggested sequence: (1) reconcile and finish current preview changes/CI; (2) complete manual native install/launch checks and address release signing/notarization decisions; (3) pick one primary user workflow and harden its end-to-end experience; (4) only then prioritize language milestones from docs/LANGUAGE_SPEC.md; (5) prepare contributor/community materials and revisit public visibility only with explicit authorization.
- Highest-value product validation gap: actual LM Studio/Liquid audio inference remains unverified. Keep the audio demo experimental unless that provider path is proven; use the offline programming example as the dependable quickstart meanwhile.
- No extra plugin is needed for this repository-grounded roadmap; next-it and plan-it cover the planning work.

## Distribution installer intent — 2026-10-12

- User requests the selected Camel logo prominently in the graphical wizard. Embed docs/images/camel-logo.png in the setup executable, decode/load it once, and show it in a persistent large branding header on all wizard screens. Refresh native installer archives while preserving existing distributions and private repository visibility.

- Completed logo branding in installer/src/ui.rs and enabled PNG decoding in ordinary builds. Native renderer screenshot at installer/target/setup-logo.png shows the selected golden Camel logo prominently in the persistent dark header. Logo is embedded directly in each setup executable and loaded once per wizard instance.
- Local compile, strict Clippy, and temporary installation of the branded Windows bundle pass. Private draft PR #1 now includes commit bb0cf6752552f6184aeddfe1b28888c3b5a21f29. Distribution run 38284259849 succeeded on Windows, macOS, and Linux, including install and repeated-upgrade checks.
- Refreshed and verified all three native Studio-inclusive archives in Camel/dist: Camel-0.2.0-preview-windows-x86_64-studio-logo-setup.zip, Camel-0.2.0-preview-macos-aarch64-studio-logo-setup.zip, and Camel-0.2.0-preview-linux-x86_64-studio-logo-setup.zip. Outer artifact digests and each installation payload checksum verified. Prior archives are preserved. A separate CLI-only branded Windows ZIP is also available.

- Follow-up user request: place macOS and Linux installer distributions in local Camel/dist too. Downloaded the successful private GitHub Actions artifacts from run 38213683842, commit d8fcfbe629186dd8f4bb46af0ce06faee2aa5b24, and extracted the inner setup ZIPs without overwriting existing archives.
- Added Camel/dist/Camel-0.2.0-preview-macos-aarch64-studio-setup.zip (Apple Silicon) and Camel/dist/Camel-0.2.0-preview-linux-x86_64-studio-setup.zip. Both include the graphical setup wizard, Camel CLI, and Camel Studio. These are genuine native builds; Intel macOS and Linux ARM64 binaries were not built by this matrix.
- Verified each downloaded outer artifact against its GitHub SHA-256 digest, every inner installation payload checksum, the matching OS/architecture manifest, the graphical launcher, and Studio's presence. All three native distribution CI jobs succeeded, including isolated install and repeated-upgrade smoke checks with PATH disabled. Native GUI appearance was observed locally on Windows only.
- Source changes remain in private draft PR https://github.com/LuminaryLabs-Dev/Camel-Language/pull/1 . Existing runtime/Studio strict CI lint failures are separate from the successful distribution jobs; repository visibility remains private.

- User requests a distributable with an installation wizard that adds Camel to PATH across operating systems, and selects a graphical setup wizard for Windows, macOS, and Linux.
- Apply atomize-it and app-paths-it. Native eframe capabilities already used by Studio fit a reusable GUI; no additional plugin is necessary. Repository remains private per the user's previous instruction.
- Implement an independent installer/ Rust crate, per-user installation defaults and persistent PATH configuration, platform/architecture-specific bundles, packaging integration, and distribution CI. Keep library data separate from installed program files and avoid modifying the workstation's actual installation/PATH during checks.

## Camel logo — 2026-10-11

- User selected the earlier abstract glass workflow-loop image as Camel's logo, despite later iterations adding legs and refining transparency.
- Copied the selected image to `docs/images/camel-logo.png` and embedded it below the title in the main `README.md`.
- The logo represents branching/await/return workflow paths with a camel-like head and hump; the user rejected the familiar side-profile cigarette-pack stance.

## Standalone library package implementation — 2026-10-11

- User replied "Not yet" to changing the private repository's visibility. Keep Camel-Language private; defer public access and do not change visibility or publish the package elsewhere without subsequent user authorization. The package already added to the private repository remains there.

- User authorized implementing the separate manager package, repository listing, and GitHub links with "Please do this." This supersedes the earlier advice-only status below.
- Created packages/camel-library-manager version 0.2.0 with Cargo metadata, Apache-2.0 LICENSE, README, lockfile, and independent Rust implementation. No root Cargo workspace was added; existing runtime and Studio target locations are preserved.
- Runtime depends on the crate by path and re-exports it as camel_audio_runtime::libraries. Removed runtime/src/libraries.rs. CLI presentation stays in the executable; catalog/install/remove/lookup ownership moves to the package. Studio uses it through its existing runtime dependency.
- Enabled reqwest rustls HTTPS, corrected @camel/<package>/<file>.camel resolution, corrected standard/core.camel's function syntax, validated cross-platform relative paths/catalog schema/unique files, made removal reject empty/root names and safely repeat, and documented serial replacement with backup recovery rather than claiming a cross-process transaction.
- Added package and library contribution guides, package listing in the local main README, standalone Windows/Linux package CI, expanded runtime CI paths, included package/catalog inputs in preview packaging, and refreshed both runtime and Studio Cargo.lock files.
- Checks: standalone package's 3 tests pass; strict Clippy and formatting pass; cargo package --offline --locked --allow-dirty successfully verifies its archive. Runtime and Studio compile. An isolated installed-library workflow returns "Camel libraries"; removing it twice succeeds. Starter source checks and --catalog file search work.
- Full runtime regression attempt passed unit tests and 2 CLI tests but failed check_validates_agent_and_audio_sources_without_model_setup because the untouched Camel/examples/agent_planner.camel:64 contains unsupported cast/Console.Log syntax and lacks a semicolon. Preserve that user-owned example; do not claim all runtime tests passed.
- Rust compiler crashed with STATUS_ACCESS_VIOLATION under broad parallel compilation; retrying with CARGO_BUILD_JOBS=2 succeeded. Rustfmt and canonicalization-based tests needed sandbox escalation; approval review allowed those actions.
- GitHub CLI authentication was unavailable in the sandbox; the connected GitHub app successfully accessed the repo with push permission. Published exactly the standalone package, its catalog/starter source/contribution guide, dedicated package CI, and a package section appended to the remote's existing README. Other untracked runtime/Studio/example/documentation work was not uploaded.
- Verified GitHub main now contains commit ee855027f2ace26cd71b5e536dddd22f13c3ca2c, whose parent is 1fcc97df515cee1df186848b9af474c5ca6a1292. Publication used a fast-forward update with an expected-head check. Local HEAD remains at the previous commit; synchronize before subsequent Git pushes while preserving local work.
- Package URL: https://github.com/LuminaryLabs-Dev/Camel-Language/tree/main/packages/camel-library-manager . Root README and package Cargo.toml were read back successfully through the connector.
- GitHub metadata reports visibility=private, including after package publication. Public anonymous access and unauthenticated GitHub source installation remain pending a repository visibility change. Ask the user to authorize exposing the entire existing private repository's tracked files/history before changing visibility. No crates.io publication or repository visibility change was performed.

## Library package architecture advice — 2026-10-11

- Follow-up intent: user asks whether the manager can be listed as a package in the repository and publicly linked. Yes: a repository-hosted Rust package can have its own directory, Cargo manifest, README, and a link from the main README once published to GitHub. Publishing on crates.io is optional and separate from listing Camel source libraries in the catalog. This question does not authorize pushing or publishing; no public package or live link has been created.

- User asked whether extracting the current library manager into a separate project package would be smart; this turn requests architecture advice, not implementation of the extraction.
- Live manifests show separate runtime and Studio crates; Studio depends on the runtime by path, and the current manager is runtime/src/libraries.rs. There is no root Cargo workspace manifest.
- Recommend a shared camel-library-manager Rust crate owning catalog discovery, package metadata, download/install/update/remove, and installed package lookup. Keep CLI presentation in the existing Camel executable, interpreter import semantics in the runtime, and future library UI in Studio.
- A separate crate provides a reusable ownership boundary without requiring a separate executable. A Cargo workspace is optional build organization; public Camel source libraries and publishing a Rust crate are separate decisions.
- Before presenting package installs as ready for public distribution, resolve known implementation gaps: GitHub HTTPS transport, matching documented nested library imports, valid starter-library syntax, version pinning, and concurrent/recoverable replacement semantics. Previous compilation and local catalog listing do not establish successful remote installation or source import execution.
- atomize-it informs the recommendation. No additional plugin helps this local Rust package boundary; no source extraction or public publication performed in this turn.

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

## New intent — 2026-10-11

The user requested library support and a library manager for discovering packages hosted in the Camel GitHub repository as an expanding public package catalog.

## Library support implementation

- Added `libraries/index.json` with a starter `standard` source library and one helper.
- Added `camel library list|search|install|update|remove` commands. The checked-out runtime prefers the repository-local catalog; a packaged CLI falls back to the public GitHub raw catalog. `--catalog` allows alternate catalog URLs.
- Catalog entries declare GitHub repository, relative source directory, and a file allowlist. Installs validate names/paths, stage files, and atomically replace an installed snapshot with rollback if replacement fails.
- Install locations are `%LOCALAPPDATA%/Camel/libraries` on Windows or `~/.camel/libraries` elsewhere; `CAMEL_HOME` overrides the base.
- `using "@camel/<name>/<file>.camel";` resolves installed library source after checking the importing file's local directory.
- Library snapshots currently track the repository's `main` branch. Version selection, dependency resolution, and signature/integrity verification remain future package-manager work.
- `cargo check --offline --locked --manifest-path runtime/Cargo.toml` passes. `camel library list` reads and displays the local starter catalog. GitHub installation could not be exercised in this restricted network environment.

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
- Added scripts/package-preview.ps1 with input preflight, staged archive creation, a flat archive root, explicit binary selection, SHA-256 file manifest and existing-archive preservation.
- Created Camel/dist/Camel-0.2.0-preview.zip (2,411,104 bytes). Verified all 33 packaged file hashes after extraction into an ignored runtime/target verification folder, checked the extracted audio source, and ran the extracted executable's offline programming example successfully.
- The original Camel-Audio-Runtime.zip and the user's existing nested extraction were not replaced. No commit, push, plugin installation, live recording or real model inference occurred in this implementation turn.

## Native IDE intent and release placement — 2026-10-10

- The user wants a simple native Rust IDE bundled with Camel, reminiscent of Python IDLE and a well-formatted Tkinter application, centered on agent orchestration rather than decorative dashboard UI.
- The user requested changes directly in the Camel repository, then asked whether development should happen in Camel-0.2.0-preview instead.
- Inspected Camel/dist/Camel-0.2.0-preview: it is an extracted distribution containing duplicated runtime sources; its Cargo manifest matches the main runtime manifest. scripts/package-preview.ps1 builds distributions from the main repository.
- Recommendation: develop the IDE alongside the canonical runtime in Camel-Language, then include it in a refreshed 0.2.0 preview distribution. Editing only the extracted release would leave the main source and subsequent bundles stale.
- Read camel-agent-programming, atomize-it, app-factory, human-app-view-expert and stack-verify-it guidance; considered native Rust GUI/file-dialog tooling. No additional plugin is necessary. IDE code has not been changed while the user clarifies release placement.

## Camel Studio implementation — 2026-10-10

- Explicit authorization: "Yeah please build the IDE in Camel-Language." Canonical source is studio/ with a path dependency on runtime/, not a fork inside an extracted distribution.
- Stack: eframe/egui 0.33.3 with native OpenGL windowing, rfd native file dialogs, Windows Segoe UI/Consolas with bundled fallbacks. No webview, Python dependency, plugin installation or external service mutation is required.
- Implemented conventional menus/toolbars, tabbed syntax-colored editor, line numbers/find/source links, project/outline, workflow argument forms, agent/tool/policy/type inspector, and live Timeline/Console/Result/Problems panes.
- Runtime gains pure AST declaration metadata, import-aware unsaved-buffer checking, shared CLI argument conversion, and opt-in observer events for workflow/agent/model/tool/microphone/log operations. Default CLI output remains unchanged; telemetry observer panics do not alter execution.
- Studio owns one hidden self-executed worker process, with a bounded JSON-lines channel and no shell commands. Stop is repeat-safe and kills/reaps only the worker; completed file writes and external model requests are not rolled back.
- Editor saves stage/flush/sync before updating document state. External snapshot conflicts and Save As collisions preserve edits; this is not a cross-process compare-and-swap guarantee. Unsaved close/quit prompts and stale-check rejection are implemented.
- Added orchestration.camel demonstrating parallel decision/risk reviewers and typed join. It checks successfully without inference. No automatic execution or recording on launch.
- Initial verification: all 50 runtime tests and 9 Studio tests passed; strict Clippy passed for both crates. Studio tests include real worker success/error/stop and headless egui layout. Native release capture built successfully.
- Workstation issue: LLVM's debug PDB writer crashes on this larger GUI link; Studio dev/test profiles disable debug symbols. Installed MSVC linker remains damaged; local builds use RUSTFLAGS=-C linker=lld-link.
- Final source verification: 50 runtime tests and 10 Studio tests pass (60 total). Strict Clippy including Studio capture feature and formatting checks pass. Added a real hidden-worker HTTP stub test for two parallel typed reviewers; this proves adapter integration, not genuine model inference.
- Reviewed actual native OpenGL screenshots after source checks settled. The initial two-frame capture caught a transient toolbar/font frame; capture support now waits for settled source state. docs/images/camel-studio.png records the real script-first IDE, and an ignored QA screenshot records Studio completing programming_core.camel through its own worker.
- The ordinary release excludes opt-in capture hooks. New/Save/Open, workflow form, declaration inspector, source diagnostics, live timeline and worker boundaries are implemented; no claim of a full manual interactive GUI/dialog pass is made. Real microphone capture/LM Studio/Liquid audio inference and Linux execution remain unverified.
- Refreshed packaging now supports -IncludeStudio, selecting Studio source/tests/Cargo lock/guide and bin/camel-studio.exe, and defaulting to a new Camel-0.2.0-preview-studio.zip rather than replacing historical archives. Bundle extraction/hash verification follows archive generation.

## LM Studio audio loading diagnosis — 2026-10-10

- User reported load-time mismatch: text model n_embd=512 versus mmproj n_embd=2048. Used debug-it to separate confirmed file inventory from inference; no additional plugin helps this model-file pairing issue.
- Read-only inspection of the default LM Studio directory C:/Users/simon/.lmstudio/models/LiquidAI/LFM2.5-Audio-1.5B-GGUF found exactly tokenizer-LFM2.5-Audio-1.5B-Q8_0.gguf (76,957,632 bytes) and mmproj-LFM2.5-Audio-1.5B-F16.gguf (458,806,624 bytes). The actual LFM2.5-Audio-1.5B main model GGUF is missing from that directory. Custom model directories and the selected load path were not inspected.
- Strong inference: LM Studio selected auxiliary tokenizer weights as its main model. The user's dimension mismatch alone does not prove the selected filename; the missing main weights are independently confirmed.
- Official LiquidAI config declares the LFM backbone hidden_size=2048. Model card assigns the main LFM2.5-Audio-1.5B-Q4_0.gguf to -m, mmproj to -mm, vocoder to -mv, and tokenizer to --tts-speaker-file; these are separate components. Q8 main weights are also published, approximately 1.25 GB.
- Recommended next step: download/select LFM2.5-Audio-1.5B-Q8_0.gguf as the main model, not tokenizer/vocoder/mmproj. Do not rename auxiliary files to disguise them as a main model. A projector's F16 versus backbone Q8 quantization does not itself establish an embedding mismatch.
- Official sources: https://huggingface.co/LiquidAI/LFM2.5-Audio-1.5B-GGUF and https://huggingface.co/LiquidAI/LFM2.5-Audio-1.5B/blob/main/config.json . The audio card documents specialized llama-liquid-audio runners; correcting file selection does not establish LM Studio audio/API support or successful Camel input_audio serving.
- No models downloaded, removed, renamed, loaded or unloaded; no runtime code was modified. Repository memory only records the diagnosis.

## Open-source GitHub positioning — 2026-10-10

- User asks how to frame Camel as a GitHub open-source project. This is positioning advice, not authorization to change GitHub metadata, commit/push, or publish a release.
- Used github and explain-it guidance; inspected the canonical README, existing Apache-2.0 LICENSE, source tracking, and GitHub's official community-health-file documentation. No additional plugin is needed for local repository positioning.
- Recommended positioning: Camel is an experimental, open-source programming language for agents, with first-class agents/tools, runtime-enforced typed contracts, bounded/concurrent execution, and a native Rust IDE. Lead with the language/runtime/Studio, not the audio demo or an unverified model integration.
- Suggested repository description: "Experimental programming language for agents, with typed contracts, bounded execution, and a native Rust IDE." Present current implementation, preview status, reproducible offline quickstart, native screenshot, limits, roadmap and contributor entry points separately.
- Important local finding: git status still reports runtime/, studio/, .github/, and current examples as untracked; git ls-files runtime studio .github produces no entries. A normal commit/push omitting these sources would not publish the new implementation. Existing remote contents/visibility could not be verified via the web fetch, so no claim is made about the remote's current state.
- No project-specific CONTRIBUTING.md, CODE_OF_CONDUCT.md or SECURITY.md found outside historical distributions/build caches. Recommend adding contributor setup/testing guidance, behavior/security policies, and focused issue templates; GitHub organization defaults may exist but were not inspected.
- Official reference: https://docs.github.com/en/communities/setting-up-your-project-for-healthy-contributions/creating-a-default-community-health-file . Keep LM Studio/Liquid audio support explicitly unverified; do not advertise production readiness, full static type checking, or OS sandboxing.
- No source implementation or public GitHub state changed; memory only records user intent and evidence-backed recommendations.
