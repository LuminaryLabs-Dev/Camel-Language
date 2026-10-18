# Camel — v.0.3.0

Camel is a programming language for agentic workflows. The current Windows source candidate and distributable identify as **v.0.3.0**. The hosted Windows CI gate remains open, so this candidate is not tagged or published as a release.

## Install and build

See [installation instructions](docs/INSTALLATION.md) and the [setup build guide](installer/README.md). From a checkout, build and verify the runtime with:

```powershell
cargo fmt --manifest-path runtime/Cargo.toml --check
cargo clippy --manifest-path runtime/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path runtime/Cargo.toml --locked
cargo build --release --manifest-path runtime/Cargo.toml --locked
```

Studio and installer have their own `Cargo.lock` files and checks; see their READMEs. CI uses `--locked` for builds and tests. The source manifests use SemVer `0.3.0`; the app and Windows distribution display `v.0.3.0`.

## v.0.3.0 readiness and known limits

This table records what the current source actually guarantees. The Camel language edition remains `0.2`; the product and executable version is `v.0.3.0`.

| Area | Current support | 0.3.0 gate / known limit |
| --- | --- | --- |
| CLI and Studio analysis | Both call the shared runtime analyzer; CLI analyzes files and Studio checks editor buffers. Stable diagnostic codes, spans, semantic/type recovery, lint warnings, and source navigation are implemented. Regression coverage compares diagnostics from file, buffer, and in-memory analysis for errors and warnings. | Add fixtures for imported-file diagnostics and CLI/Studio presentation. File labels are API-specific; codes, messages, phases, and ranges are the shared contract. |
| Bundle rebuild, hashes, upgrades | Rebuilt [the current Windows x86_64 Studio bundle](Camel/dist/Camel-v.0.3.0-windows-x86_64-studio-setup.zip). All 94 `BUNDLE_MANIFEST.json` entries verify. Two unattended installs into a workspace-local test folder succeeded, including upgrade; installed CLI, Studio, and alias hashes match the bundle. | Candidate identifies as **v.0.3.0**. Hosted Windows packaging CI remains pending. Hashes detect corruption; they are not publisher signatures. |
| Approval for consequential effects | CLI `--approve` flags and Studio's per-run approval dialog gate model calls, microphone capture, file reads/writes, network access, and shell capability before dispatch. Direct and agent-invoked tools share the check. | Blocking host calls only stop after returning. Trusted embedded hosts using the default `None` approval setting retain compatibility and must opt into enforcement. Approval is not an OS sandbox. |
| Cancellation, resume, deadline | Runtime exposes cooperative cancellation/deadlines and explicit `checkpoint("label");` workflow boundaries. CLI `--checkpoint <path>` saves JSON-safe scopes, logs, budgets, elapsed active time, and cassette position; `--resume <path>` continues after that statement and rejects stale source fingerprints. When continuing a cassette, already-recorded post-checkpoint events replay before new events are recorded. | Checkpoints must be direct statements in the entry workflow; active spawned tasks and audio/callable/task values cannot be checkpointed. Effects before a boundary are not rolled back; wall-clock downtime is excluded from elapsed deadline accounting. |
| Replayable evaluations | `--record-cassette <path>` persists ordered model, registered-tool, environment, clock, audio-load, and microphone results; `--replay-cassette <path>` verifies the source fingerprint, each request, and event order, then replays without repeating those effects. `--evaluation-report <path>` emits stable JSON with source/cassette fingerprints, result, logs, and token/tool counts. | Parallel spawned effects can be schedule-sensitive. A crash between an external effect and its cassette flush can leave that effect unrecorded. Replay covers registered tool boundaries, not arbitrary hidden effects inside custom host code. |
| Budget visibility | CLI accepts `--tool-calls`, `--tokens`, and `--timeout-seconds`; Studio shows live token and tool-call progress bars, elapsed time against the deadline, and freezes final elapsed time when a run ends. Displayed limits come from runtime defaults: 256 calls, 32,768 reported tokens, and 600 seconds. Runtime accounts for provider-reported token usage and rejects unsupported providers before requests. | Token enforcement depends on providers reporting `usage.total_tokens`; time display is elapsed time, not a prediction of remaining runtime. |
| Typed evidence and handoffs | `AgentResult` includes a required typed `EvidenceProvenance` value: agent, model, full source span, program fingerprint, and tool names. Passing the complete result to another agent preserves that provenance in its serialized prompt. | Passing only `AgentResult.data` intentionally drops the envelope. Provenance identifies declared source and tool origins; it is not a cryptographic signature or proof that a model's claims are true. |
| Provider capability negotiation | Providers declare tool-call, structured-output, and token-usage support; the runtime checks requirements before sending requests. | Capability declarations are static and do not probe model-specific support. Live compatibility still needs provider integration coverage. |
| Reproducible packages | Runtime, Studio, installer, and the library manager each have a committed `Cargo.lock`; CI uses locked dependency resolution. Schema 3 source-library catalog entries pin immutable commits and SHA-256 file digests. `camel library lock`, `verify-lock`, and `install-locked` resolve exact direct/transitive versions in deterministic order. The checked-in [Camel.lock](libraries/Camel.lock) pins the bundled catalog. | Digests detect byte drift, not publisher identity. A multi-package install is ordered and rerunnable, but not transactional; earlier packages can remain installed if a later download fails. Cargo lockfiles remain per crate, not one workspace lock. |
| Coverage and Windows CI | Focused regression tests cover checkpoint restore, cassette replay/report stability, provenance handoffs, imported diagnostic source spans, and Studio navigation/highlighting. CI runs formatting, strict Clippy, test suites, native packaging, lock verification, and installer/upgrade smoke checks on Windows. | The hosted Windows workflow must pass on the exact candidate commit; local checks cannot substitute for that run. Linux and macOS releases are out of the current 0.3.0 target. |

### Changes tracked for 0.3.0

- Shared CLI and Studio diagnostics with stable codes, source ranges, recovery, and warning-only unused-name/import lint.
- Rust formatting and Clippy fixes for the shared analysis implementation; parser nesting now returns a bounded diagnostic rather than overflowing the process stack.
- Added analysis parity coverage and expanded distribution CI triggers to relevant source, lock, library, documentation, and example changes; the Windows distribution workflow runs formatting, strict Clippy, and crate test suites before packaging.
- Made Model Manager contents scroll as one window; repository results are bounded and collapsible, every GGUF file appears in a selectable full-width list below them, single-file repos auto-select, and load/import status stays beside the files.
- Fixed Model Manager terminal updates being overwritten as unexpected stops after successful repository lookup, search, import, or model operations; repository GGUF files now remain available after metadata loads.
- Fixed Hugging Face repository metadata URLs to avoid an empty path segment between `/models/` and the repository owner; previously this produced 404s such as `/api/models//LiquidAI/...`.
- Added cooperative cancellation and total run deadlines to the runtime, global tool-call and provider-reported token budgets, and provider feature checks before model requests. CLI exposes `--tool-calls`, `--tokens`, and `--timeout-seconds`.
- Added per-run capability approval in Studio and explicit CLI `--approve` flags. Approval covers model calls, microphone capture, file reads and writes, network access, and shell capability; unapproved effects fail before dispatch. Trusted embedded hosts must set `approved_capabilities` to opt into enforcement.
- Studio now displays live token/tool progress and elapsed time against runtime limits, using the same `Limits::default()` values as the worker; final elapsed time remains visible after success, failure, or cancellation.
- Added explicit top-level workflow checkpoints with source validation, JSON-safe scope restore, preserved logs/budgets, and cassette cursor restore/continue support.
- Added persistent ordered cassettes for model, tool, environment, clock, and audio boundaries, plus no-side-effect replay and deterministic evaluation report files. Replay strictly checks ordered inputs and rejects unused cassette entries.
- Studio's per-run dialog now exposes checkpoint/resume, cassette record/replay, and evaluation-report paths through its worker; replay uses the same source-bound runtime cassette implementation as CLI.
- Added typed `AgentResult.provenance` / `EvidenceProvenance` metadata for agent, model, call site, source fingerprint, and invoked tool names; complete result handoffs retain the envelope.
- Added direct coverage for checkpoint scope restoration, model replay without a provider and stale-source rejection, byte-stable evaluation reports, provenance-bearing agent handoffs, imported source spans, and Studio Problems navigation/highlighting.
- Local checks for the new library lock format pass: deterministic dependency ordering, version mismatch rejection, checksum verification, atomic lock round-trip, CLI generation, and offline verification. A locked install verifies every downloaded file before replacing it.
- Source manifests use Cargo SemVer **0.3.0** and the CLI, Studio, installer, and Windows archive display **v.0.3.0**. The hosted Windows workflow has not run for this uncommitted candidate; it has not been tagged or published.
- Latest Windows local checks: all four crate suites pass (107 tests total), including Runtime and Studio local mock-server tests when loopback access is enabled; formatting and strict Clippy pass across all crates; release builds succeed for Runtime, Studio, and installer; generated `Camel.lock` matches byte-for-byte. The v.0.3.0 Windows bundle verifies all 94 manifest hashes and passes two install/upgrade runs. The hosted Windows workflow remains unrun for this uncommitted candidate.

## v.0.3.0 stress harness

[`scripts/stress-camel-0.3.ps1`](scripts/stress-camel-0.3.ps1) runs bounded offline probes for step, tool-call, and total-time exhaustion, then compiles and runs the persistent five-agent program in [`Camel/examples/stress_five_models.camel`](Camel/examples/stress_five_models.camel). It checks that every model returns the typed `Review` record and respects the run-wide token and deadline limits. All five model IDs must be served by the configured OpenAI-compatible endpoint (`CAMEL_LM_ENDPOINT`; defaults to `http://localhost:1234`).

```powershell
pwsh -File scripts/stress-camel-0.3.ps1 `
  -Model1 "model-id-one" -Model2 "model-id-two" -Model3 "model-id-three" `
  -Model4 "model-id-four" -Model5 "model-id-five"
```

Set `CAMEL_LM_ENDPOINT` to the server root or `/v1` URL. Set `CAMEL_LM_TOKEN` when the server requires authentication; set `CAMEL_LM_TIMEOUT_SECONDS` to change the per-request timeout (default 300 seconds). The script passes the five `-Model1` through `-Model5` values into `CAMEL_STRESS_MODEL_1` through `CAMEL_STRESS_MODEL_5` for the Camel process. Each ID must identify a model available from that endpoint. `-TokenBudget` limits total reported tokens for the run, `-TimeoutSeconds` limits the full run, and `-PromptChars` sets the shared input size. The five agents allow no tools.

Use `-ValidateOnly` to run the offline exhaustion probes and compile the checked-in five-model program without making model requests. Increase `-PromptChars`, `-TokenBudget`, or `-TimeoutSeconds` to apply more load. The harness does not exercise checkpoint/resume, cassette replay, provenance handoffs, or library-lock behavior.

## Repository packages

| Package | Purpose | Documentation |
| --- | --- | --- |
| `camel-library-manager` | Shared Rust package for catalog discovery, HTTPS source-library installation, updates, removal, and installed-source lookup. | [Package guide](packages/camel-library-manager/README.md) |

Browse [repository packages](packages/README.md) and the [Camel source-library catalog](libraries/README.md). The library manager has not been published to crates.io. Repository visibility controls anonymous access; source-library installation currently expects publicly readable repositories.

## Language and tools

The [language specification](docs/LANGUAGE_SPEC.md) and executable conformance fixtures describe the implemented language. See [runtime](runtime/README.md), [Studio](studio/README.md), [installer](installer/README.md), and [distribution workflow](.github/workflows/distribution.yml) for component behavior, checks, and operational limits.
