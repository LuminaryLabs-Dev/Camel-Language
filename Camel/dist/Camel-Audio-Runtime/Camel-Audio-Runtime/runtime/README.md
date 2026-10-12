# Camel Audio Runtime

This crate is the first source-backed runtime slice for the recovered Camel repository. It executes the audio note summarizer workflow and leaves the missing general Camel compiler/runtime separate.

## Responsibilities

- Validate the audio workflow's required declarations.
- Resolve the workflow audio path from the caller, with source-relative fallback.
- Reject missing, empty, or unsupported audio files before network access.
- Encode audio as base64 `input_audio` content.
- Call LM Studio once for transcription.
- Call LM Studio once for topic-focused summarization.
- Parse the summary into the typed `NoteSummary` shape, with a visible fallback when the model returns plain text.

## Commands

```powershell
cargo test --manifest-path .\runtime\Cargo.toml
cargo run --manifest-path .\runtime\Cargo.toml -- check .\Camel\examples\audio_note_summarizer.camel
cargo run --manifest-path .\runtime\Cargo.toml -- .\Camel\examples\audio_note_summarizer.camel --lm --json -- .\notes\meeting.wav "Q4 launch decisions"
```

The host uses `CAMEL_LM_ENDPOINT` or defaults to `http://localhost:1234`, then posts to `/v1/chat/completions`. Both `http://localhost:1234` and `http://localhost:1234/v1` are accepted. This focused crate intentionally supports local HTTP; add a TLS client feature only when a remote HTTPS endpoint is in scope. `CAMEL_AUDIO_MODEL`, `CAMEL_SUMMARY_MODEL`, `CAMEL_LM_TOKEN`, `LM_API_TOKEN`, and `CAMEL_LM_TIMEOUT_SECONDS` are supported.

The request uses OpenAI-compatible `input_audio` content. Whether a specific LM Studio build and Liquid audio model accept that content is a live integration concern; the unit tests establish the local encoding and request contract, not successful model inference.
