# Camel — 0.3.0 preview in progress

Camel is a programming language for agentic workflows. The current source preview and distributable are still **0.2.0-preview**; the version will move to 0.3.0 only after the readiness gates below are closed. Preview distributions are not public until access is authorized.

## Install and build

See [installation instructions](docs/INSTALLATION.md) and the [setup build guide](installer/README.md). From a checkout, build and verify the runtime with:

```powershell
cargo fmt --manifest-path runtime/Cargo.toml --check
cargo clippy --manifest-path runtime/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path runtime/Cargo.toml --locked
cargo build --release --manifest-path runtime/Cargo.toml --locked
```

Studio and installer have their own `Cargo.lock` files and checks; see their READMEs. CI uses `--locked` for builds and tests. Do not update published version numbers or produce a public release until the 0.3.0 readiness gates are reviewed.

## 0.3.0 readiness and known limits

This table records what the current source actually guarantees. “Next” items are release work, not features already promised by 0.2.0.

| Area | Current support | 0.3.0 gate / known limit |
| --- | --- | --- |
| CLI and Studio analysis | Both call the shared runtime analyzer; CLI analyzes files and Studio checks editor buffers. Stable diagnostic codes, spans, semantic/type recovery, lint warnings, and source navigation are implemented. Regression coverage compares diagnostics from file, buffer, and in-memory analysis for errors and warnings. | Add fixtures for imported-file diagnostics and CLI/Studio presentation. File labels are API-specific; codes, messages, phases, and ranges are the shared contract. |
| Bundle rebuild, hashes, upgrades | The package script creates per-file SHA-256 manifests and prints the archive SHA-256. The installer verifies payload hashes and stages upgrades with recovery; distribution CI rebuilds and installs the package twice when bundled files change. The latest local Windows rehearsal verified all 92 archive entries and two installs. | Cross-platform packaging remains a CI responsibility. Hashes detect corruption; they are not publisher signatures. |
| Approval for consequential effects | Agent policies validate declared tool capabilities before provider requests. | Human approval of a per-run capability set is not yet wired into CLI or Studio. Workflows remain trusted code, not an OS sandbox. |
| Cancellation, resume, deadline | Runtime exposes a shared cooperative cancellation token and checks it at execution boundaries. Studio Stop signals that token through the worker; runs have a total deadline (default 10 minutes; CLI configurable), plus step, function-depth, task, agent-turn, and tool-call limits. | Blocking host calls only stop after returning. No resumable checkpoints exist; completed effects are not undone. |
| Replayable evaluations | Automated tests use fake providers and recorders. | There is no persisted request/response cassette, replay command, or deterministic evaluation report. Live model responses remain nondeterministic. |
| Budget visibility | CLI accepts `--tool-calls`, `--tokens`, and `--timeout-seconds`; defaults are 256 calls, 32,768 reported tokens, and 600 seconds. Runtime accounts for provider-reported token usage and rejects unsupported providers before requests. | Studio does not yet expose remaining token/time/tool budgets. Token enforcement depends on providers reporting `usage.total_tokens`. |
| Typed evidence and handoffs | Camel has typed record outputs and typed task results. | Handoffs do not yet carry provenance-bearing evidence bundles or a standard evidence schema. |
| Provider capability negotiation | Providers declare tool-call, structured-output, and token-usage support; the runtime checks requirements before sending requests. | Capability declarations are static and do not probe model-specific support. Live compatibility still needs provider integration coverage. |
| Reproducible packages | Runtime, Studio, installer, and the library manager each have a committed `Cargo.lock`; CI uses locked dependency resolution. Source-library catalog entries pin a semantic version and Git revision. | Source-library installs do not yet verify content digests or resolve transitive library dependencies. Cargo lockfiles are per crate, not one workspace lock. |

### Changes tracked for 0.3.0

- Shared CLI and Studio diagnostics with stable codes, source ranges, recovery, and warning-only unused-name/import lint.
- Rust formatting and Clippy fixes for the shared analysis implementation; parser nesting now returns a bounded diagnostic rather than overflowing the process stack.
- Added analysis parity coverage and expanded distribution CI triggers to changes in bundled runtime, Studio, library, docs, and example files.
- Made Model Manager contents scroll as one window; repository results are bounded and collapsible, every GGUF file appears in a selectable full-width list below them, single-file repos auto-select, and load/import status stays beside the files.
- Fixed Model Manager terminal updates being overwritten as unexpected stops after successful repository lookup, search, import, or model operations; repository GGUF files now remain available after metadata loads.
- Fixed Hugging Face repository metadata URLs to avoid an empty path segment between `/models/` and the repository owner; previously this produced 404s such as `/api/models//LiquidAI/...`.
- Added cooperative cancellation and total run deadlines to the runtime, global tool-call and provider-reported token budgets, and provider feature checks before model requests. CLI exposes `--tool-calls`, `--tokens`, and `--timeout-seconds`.
- Rebuilt a local **0.2.0-preview** Windows bundle with Studio; verified all 91 file hashes, the archive SHA-256, and two successful installs/upgrades. This is a packaging rehearsal, not a 0.3.0 release.
- Human approval, Studio cancellation wiring and budget display, explicit checkpoints, persisted replay artifacts, typed evidence provenance, content digests, and transitive library locks remain open release gates.
- Final local preparation rehearsal for the current source passed formatting, strict Clippy, all four crate test suites, release builds, CLI smoke checks, all 92 bundle hashes, and repeated Windows install/upgrade. It still packages as **0.2.0-preview**. The latest GitHub PR run failed at formatting before tests/builds; local formatting now passes, but CI must pass on a committed candidate before release.

## Repository packages

| Package | Purpose | Documentation |
| --- | --- | --- |
| `camel-library-manager` | Shared Rust package for catalog discovery, HTTPS source-library installation, updates, removal, and installed-source lookup. | [Package guide](packages/camel-library-manager/README.md) |

Browse [repository packages](packages/README.md) and the [Camel source-library catalog](libraries/README.md). The library manager has not been published to crates.io. Repository visibility controls anonymous access; source-library installation currently expects publicly readable repositories.

## Language and tools

The [language specification](docs/LANGUAGE_SPEC.md) and executable conformance fixtures describe the implemented language. See [runtime](runtime/README.md), [Studio](studio/README.md), [installer](installer/README.md), and [distribution workflow](.github/workflows/distribution.yml) for component behavior, checks, and operational limits.
