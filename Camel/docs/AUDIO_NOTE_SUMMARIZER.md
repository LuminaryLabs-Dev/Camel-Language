# Audio Note Summarizer — Camel v.0.3.0

The audio inference implementation lives in the separately versioned `libraries/audio` source package. `Camel/examples/audio_note_summarizer.camel` imports the checkout copy for local development; `Camel/examples/audio_note_summarizer_installed.camel` imports the installed package:

1. `Main(topic, durationSeconds)` executes `record(durationSeconds)` to capture the default microphone into an in-memory PCM WAV.
2. The `audio` library's `AudioTranscriber` uses its selected audio model and returns spoken text.
3. Its `NoteSummarizer` uses its selected text model and returns JSON matching the library's `NoteSummary` record.
4. The library returns an `AudioNote` record containing both transcript and topic-focused summary.

This captures a recording and then transcribes it. It is **not continuous streaming transcription**.

## Requirements and limitation

Use the v.0.3.0 source runtime or executable, not the recovered alpha binary. A default microphone must be available and operating-system recording permission enabled.

The runtime provides generic audio input values and capture. Transcription and summarization prompts, agents, model defaults, and response records belong to the independently versioned `audio` library. Its catalog version and Git revision can advance without a runtime release. Install or update it with `camel library install audio` and `camel library update audio`, then check the installed-import example. The repository-local example imports the library source directly. This repository is private, so remote install requires an accessible catalog and source repository; local checkout imports remain available.

The configured local server must expose the requested model IDs and accept an OpenAI-compatible `input_audio` content part for the audio model. Camel emits that payload, but **live LM Studio/Liquid audio inference has not been verified**. Installing a model does not prove the server can serve its audio input. If the server rejects `input_audio`, an audio-capable provider adapter is still needed; Camel cannot make a text/image-only server accept audio.

The tests inject a recorder and provider, proving source execution and payload construction without recording the user or claiming real inference.

## Run from Camel-Language

```powershell
$env:CAMEL_LM_ENDPOINT = "http://localhost:1234"
$env:CAMEL_AUDIO_MODEL = "LiquidAI/LFM2.5-Audio-1.5B-GGUF"
$env:CAMEL_SUMMARY_MODEL = "LiquidAI/LFM2.5-1.2B-Thinking"

# Only if this workstation's MSVC linker is broken and LLVM is installed:
$env:RUSTFLAGS = "-C linker=lld-link"

cargo test --manifest-path .\runtime\Cargo.toml --locked
cargo run --manifest-path .\runtime\Cargo.toml -- check .\Camel\examples\audio_note_summarizer.camel
```

For the independently installed package:

```powershell
cargo run --manifest-path .\runtime\Cargo.toml -- library install audio --catalog .\libraries\index.json
cargo run --manifest-path .\runtime\Cargo.toml -- check .\Camel\examples\audio_note_summarizer_installed.camel
```

Run the local-source version:

```powershell

# Source parameters: topic, then duration in seconds.
cargo run --manifest-path .\runtime\Cargo.toml -- .\Camel\examples\audio_note_summarizer.camel --json -- "Luminary Initiatives" 30
```

To record until Enter:

```powershell
cargo run --manifest-path .\runtime\Cargo.toml -- record .\Camel\examples\audio_note_summarizer.camel --until-enter --json -- "Luminary Initiatives"
```

To use a duration override and retain the recording:

```powershell
New-Item -ItemType Directory -Force .\notes | Out-Null
cargo run --manifest-path .\runtime\Cargo.toml -- record .\Camel\examples\audio_note_summarizer.camel --seconds 60 --save-recording .\notes\luminary-initiatives.wav --json -- "Luminary Initiatives"
```

Recording destinations need an existing parent directory. Saving stages a complete WAV before replacing the destination; repeated saves intentionally replace that file.

## Build once

```powershell
cargo build --release --manifest-path .\runtime\Cargo.toml --locked
& .\runtime\target\release\camel.exe record .\Camel\examples\audio_note_summarizer.camel --seconds 30 --json -- "Q4 launch decisions"
```

`camel-audio-runtime.exe` remains an alias. `--lm` is accepted for compatibility but is unnecessary. Models, prompts and typed responses are defined by the Camel source.

If using the preview ZIP, extract into a fresh folder. It contains `runtime/Cargo.toml` at the archive root, plus `bin/camel.exe`. Run commands from that extracted root, not an old extra-nested audio bundle.

## File-input diagnostic workflow

The same source also declares `FromFile`, calling the same `Summarize` function:

```powershell
cargo run --manifest-path .\runtime\Cargo.toml -- .\Camel\examples\audio_note_summarizer.camel --workflow FromFile --json -- .\notes\meeting.wav "Q4 launch decisions"
```

Relative audio paths prefer the caller's current directory, falling back to the source directory. Absolute audio paths are allowed for trusted workflow code. This file mode must now select `--workflow FromFile`; the old CLI that ignored the workflow's declared arguments is no longer used.

## Output and policy

`--json` emits `{"value":{"transcript":{...},"summary":{...}},"logs":[...]}`. Both typed records are validated. Invalid summary JSON fails visibly; the new language path never silently substitutes an untyped summary. A leading closed `<think>...</think>` section or fenced JSON may be removed before parsing.

The policy permits no model-requested tools. Its `allowNetwork=false` controls tool networking, not the configured model endpoint, explicit workflow microphone capture, or explicit source audio loading. This is not an OS sandbox.

The default capture uses the device's native sample rate and channel count, encoded as PCM16 WAV. No model-specific resampling is implemented. WAV is the initial diagnostic format; serving compatibility remains a provider responsibility.
