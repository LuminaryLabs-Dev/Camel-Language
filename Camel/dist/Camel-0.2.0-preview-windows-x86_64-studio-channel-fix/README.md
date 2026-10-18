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
| Bundle rebuild, hashes, upgrades | The package script creates per-file SHA-256 manifests and prints the archive SHA-256. The installer verifies payload hashes and stages upgrades with recovery; distribution CI rebuilds and installs the package twice when bundled files change. A local Windows rehearsal verified all 91 archive entries and two installs. | Cross-platform packaging remains a CI responsibility. Hashes detect corruption; they are not publisher signatures. |
| Approval for consequential effects | Agent policies validate requested tool capabilities before the first provider request. | Direct workflow tool calls have no human approval callback. File writes and network calls must be gated consistently at the host boundary before claiming approval protection. Workflows are trusted code, not an OS sandbox. |
| Cancellation, resume, deadline | Studio Stop terminates its worker process. Provider requests have per-request timeouts; the runtime bounds steps, function depth, tasks, agent turns, and tool calls. | Stop is process termination, not cooperative cancellation; completed effects are not undone. There are no resumable checkpoints or total run deadline. |
| Replayable evaluations | Automated tests use fake providers and recorders. | There is no persisted request/response cassette, replay command, or deterministic evaluation report. Live model responses remain nondeterministic. |
| Budget visibility | Step/task/call-depth limits and agent turn/tool-call caps exist. Studio shows elapsed operation time and tool events. | Token usage is not reported; elapsed time is observational rather than a total run deadline. A unified remaining-budget view is not implemented. |
| Typed evidence and handoffs | Camel has typed record outputs and typed task results. | Handoffs do not yet carry provenance-bearing evidence bundles or a standard evidence schema. |
| Provider capability negotiation | Tool registration declares host capabilities, and agent policies validate those capabilities before contacting a model. | Model providers expose no capability-negotiation contract. Unsupported provider features may fail during a request. |
| Reproducible packages | Runtime, Studio, installer, and the library manager each have a committed `Cargo.lock`; CI uses locked dependency resolution. Source-library catalog entries pin a semantic version and Git revision. | Source-library installs do not yet verify content digests or resolve transitive library dependencies. Cargo lockfiles are per crate, not one workspace lock. |

### Changes tracked for 0.3.0

- Shared CLI and Studio diagnostics with stable codes, source ranges, recovery, and warning-only unused-name/import lint.
- Rust formatting and Clippy fixes for the shared analysis implementation; parser nesting now returns a bounded diagnostic rather than overflowing the process stack.
- Added analysis parity coverage and expanded distribution CI triggers to changes in bundled runtime, Studio, library, docs, and example files.
- Made Model Manager contents scroll as one window; repository results are bounded and collapsible, every GGUF file appears in a selectable full-width list below them, single-file repos auto-select, and load/import status stays beside the files.
- Fixed Model Manager terminal updates being overwritten as unexpected stops after successful repository lookup, search, import, or model operations; repository GGUF files now remain available after metadata loads.
- Rebuilt a local **0.2.0-preview** Windows bundle with Studio; verified all 91 file hashes, the archive SHA-256, and two successful installs/upgrades. This is a packaging rehearsal, not a 0.3.0 release.
- Approval, cooperative cancellation, checkpoints, total deadlines, replay artifacts, token budgets, evidence provenance, provider negotiation, and library content locks remain explicit follow-up work until implemented and verified.

## Repository packages

| Package | Purpose | Documentation |
| --- | --- | --- |
| `camel-library-manager` | Shared Rust package for catalog discovery, HTTPS source-library installation, updates, removal, and installed-source lookup. | [Package guide](packages/camel-library-manager/README.md) |

Browse [repository packages](packages/README.md) and the [Camel source-library catalog](libraries/README.md). The library manager has not been published to crates.io. Repository visibility controls anonymous access; source-library installation currently expects publicly readable repositories.

## Language and tools

The [language specification](docs/LANGUAGE_SPEC.md) and executable conformance fixtures describe the implemented language. See [runtime](runtime/README.md), [Studio](studio/README.md), [installer](installer/README.md), and [distribution workflow](.github/workflows/distribution.yml) for component behavior, checks, and operational limits.
