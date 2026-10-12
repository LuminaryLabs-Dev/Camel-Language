# Audio Note Summarizer

`examples/audio_note_summarizer.camel` is a two-agent local workflow:

1. `AudioTranscriber` sends the supplied audio path to `LiquidAI/LFM2.5-Audio-1.5B-GGUF` and returns a typed transcript.
2. `NoteSummarizer` sends that transcript to `LiquidAI/LFM2.5-1.2B-Thinking` and returns a topic-focused note summary.

The workflow accepts a topic so summaries can prioritize a project, person, decision, customer, or any other relevant field.

## Prerequisites

- LM Studio is installed and its local server is running on `http://localhost:1234`.
- Both requested models are downloaded and available to LM Studio.
- The new `runtime` Cargo host includes audio input support that maps `audio = [path]` to an OpenAI-compatible `input_audio` request part.
- The audio model accepts the supplied format; WAV is the safest first test.

The new runtime first resolves a relative audio path from the caller's working directory, then falls back to the Camel source file's directory. It validates the extension, base64-encodes the file, and sends it to `/v1/chat/completions`. The older bundled `camel.exe` remains a reference artifact and does not include this audio runtime.

## Build and run the audio runtime

From `Camel-Language`:

```powershell
$env:CAMEL_LM_ENDPOINT = "http://localhost:1234"
$env:CAMEL_AUDIO_MODEL = "LiquidAI/LFM2.5-Audio-1.5B-GGUF"
$env:CAMEL_SUMMARY_MODEL = "LiquidAI/LFM2.5-1.2B-Thinking"

cargo test --manifest-path .\runtime\Cargo.toml
cargo run --manifest-path .\runtime\Cargo.toml -- check .\Camel\examples\audio_note_summarizer.camel
cargo run --manifest-path .\runtime\Cargo.toml -- .\Camel\examples\audio_note_summarizer.camel --lm --json -- .\notes\meeting.wav "Q4 launch decisions"
```

For a reusable executable:

```powershell
cargo build --release --manifest-path .\runtime\Cargo.toml
& .\runtime\target\release\camel-audio-runtime.exe .\Camel\examples\audio_note_summarizer.camel --lm --json -- .\notes\meeting.wav "Q4 launch decisions"
```

The `--lm` flag is accepted explicitly for Camel host compatibility; this runtime always uses LM Studio's OpenAI-compatible endpoint.

The first argument is the audio path. The second argument is the prioritization topic. Relative paths are resolved from the current directory first, then relative to the Camel source file.

## Optional model loading

LM Studio can download or load models before execution. Model identifiers may display with a quantization or variant suffix in the local model list; use the exact identifier reported by `lms ls` if the default names do not resolve.

```powershell
lms server start
lms get "LiquidAI/LFM2.5-Audio-1.5B-GGUF"
lms get "LiquidAI/LFM2.5-1.2B-Thinking"
lms ls
lms load "LiquidAI/LFM2.5-Audio-1.5B-GGUF"
lms load "LiquidAI/LFM2.5-1.2B-Thinking"
```

For a custom runtime executable, set its path once:

```powershell
$camel = "C:\path\to\camel-audio-runtime.exe"
& $camel check .\Camel\examples\audio_note_summarizer.camel
& $camel .\Camel\examples\audio_note_summarizer.camel --trace --json -- .\notes\meeting.wav "security risks"
```

`--trace` includes the execution summary. `--json` makes the `SUMMARY_JSON` log easier to pipe into another tool. The alpha workflow entry point is void, so the script emits the typed summary as JSON rather than returning it directly.
