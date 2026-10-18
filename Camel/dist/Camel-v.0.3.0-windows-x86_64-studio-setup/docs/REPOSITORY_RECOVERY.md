# Repository Recovery Notes

## Recovery date

2026-09-04

## What was recoverable

- The configured remote is `https://github.com/LuminaryLabs-Dev/Camel-Language.git`.
- The remote exposes only `main` at the initial commit.
- The initial commit contains `.gitattributes`, `LICENSE`, and `README.md`.
- Those tracked baseline files have been restored in this checkout.
- The checkout contains a compiled Windows distribution under `Camel/dist/Camel-Trees-Demo/`.
- The distribution includes a self-contained `encyclopedia.camel` example, launch script, bundle README, and `camel.exe`.
- The included executable validates the example with `camel check`: one workflow, four agents, and two tools.
- The design documentation describes the intended parser, runtime, ToolRegistry, workflows, policies, LM Studio integration, structured records, and bounded execution model.

## What was not recoverable

- No Rust/Cargo source tree is present in the checkout.
- No `Cargo.toml`, parser implementation, runtime implementation, or test suite is present.
- No additional branches, tags, reflog entries, or unreachable Git objects contain the missing source.
- The compiled executable is not a substitute for maintainable source and should remain a reference artifact only.

## Current source of truth

Until the original source is supplied or recovered from another backup, the authoritative material is:

1. `Camel/docs/camel-article.md` for the intended language/runtime architecture.
2. `Camel/dist/Camel-Trees-Demo/encyclopedia.camel` for the currently demonstrated syntax.
3. `Camel/docs/LM_STUDIO.md` for the LM Studio integration contract.
4. `Camel/dist/Camel-Trees-Demo/camel.exe` for the observed `0.1.0-alpha` CLI behavior.

## Recommended next recovery action

Locate the original development checkout, build archive, CI artifact, or contributor backup that contains the Rust source. Do not reimplement the compiler from the executable until those sources have been ruled out; if reimplementation becomes necessary, begin with a versioned language specification and parser/runtime/end-to-end tests for one small vertical slice.
