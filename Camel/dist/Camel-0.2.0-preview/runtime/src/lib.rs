pub mod language;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const DEFAULT_ENDPOINT: &str = "http://localhost:1234";

#[derive(Debug)]
pub enum RuntimeError {
    Io(std::io::Error),
    Http(reqwest::Error),
    Json(serde_json::Error),
    InvalidInput(String),
    Audio(String),
    Model(String),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "file error: {error}"),
            Self::Http(error) => write!(f, "LM Studio request failed: {error}"),
            Self::Json(error) => write!(f, "invalid JSON: {error}"),
            Self::InvalidInput(message) => write!(f, "invalid input: {message}"),
            Self::Audio(message) => write!(f, "audio error: {message}"),
            Self::Model(message) => write!(f, "model response error: {message}"),
        }
    }
}

impl std::error::Error for RuntimeError {}

impl From<std::io::Error> for RuntimeError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<reqwest::Error> for RuntimeError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

impl From<serde_json::Error> for RuntimeError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Debug, Clone)]
pub struct AudioInput {
    pub path: PathBuf,
    pub format: String,
    pub data_base64: String,
}

impl AudioInput {
    pub fn from_path(source_dir: &Path, requested_path: &str) -> Result<Self, RuntimeError> {
        if requested_path.trim().is_empty() {
            return Err(RuntimeError::InvalidInput(
                "audio path cannot be empty".to_string(),
            ));
        }

        let requested = Path::new(requested_path);
        let joined = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            let working_directory_path = std::env::current_dir()?.join(requested);
            if working_directory_path.exists() {
                working_directory_path
            } else {
                source_dir.join(requested)
            }
        };
        let path = joined.canonicalize().map_err(|error| {
            RuntimeError::InvalidInput(format!(
                "cannot resolve audio file '{}': {error}",
                joined.display()
            ))
        })?;
        let metadata = fs::metadata(&path)?;
        if !metadata.is_file() {
            return Err(RuntimeError::InvalidInput(format!(
                "audio path is not a file: {}",
                path.display()
            )));
        }
        if metadata.len() == 0 {
            return Err(RuntimeError::InvalidInput(format!(
                "audio file is empty: {}",
                path.display()
            )));
        }

        let format = audio_format(&path)?;
        let data_base64 = BASE64.encode(fs::read(&path)?);
        Ok(Self {
            path,
            format,
            data_base64,
        })
    }

    pub fn from_wav_bytes(display_name: &str, wav_bytes: Vec<u8>) -> Result<Self, RuntimeError> {
        if wav_bytes.is_empty() {
            return Err(RuntimeError::InvalidInput(
                "recorded audio is empty".to_string(),
            ));
        }
        Ok(Self {
            path: PathBuf::from(display_name),
            format: "wav".to_string(),
            data_base64: BASE64.encode(wav_bytes),
        })
    }

    pub fn save_wav(&self, path: &Path) -> Result<(), RuntimeError> {
        let wav_bytes = BASE64.decode(&self.data_base64).map_err(|error| {
            RuntimeError::Audio(format!("cannot decode captured WAV data: {error}"))
        })?;
        use std::io::Write;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut pending = tempfile::NamedTempFile::new_in(parent)?;
        pending.write_all(&wav_bytes)?;
        pending.as_file().sync_all()?;
        pending
            .persist(path)
            .map_err(|e| RuntimeError::Io(e.error))?;
        Ok(())
    }

    fn content_part(&self) -> Value {
        json!({
            "type": "input_audio",
            "input_audio": {
                "data": self.data_base64,
                "format": self.format
            }
        })
    }
}

fn audio_format(path: &Path) -> Result<String, RuntimeError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .ok_or_else(|| {
            RuntimeError::InvalidInput("audio file needs a supported extension".to_string())
        })?;

    match extension.as_str() {
        "wav" | "mp3" | "m4a" | "ogg" | "flac" | "webm" => Ok(extension),
        _ => Err(RuntimeError::InvalidInput(format!(
            "unsupported audio extension '.{extension}'; use wav, mp3, m4a, ogg, flac, or webm"
        ))),
    }
}

pub fn record_microphone(duration: Option<u64>) -> Result<AudioInput, RuntimeError> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or_else(|| {
        RuntimeError::Audio("no default microphone input device is available".to_string())
    })?;
    let supported = device.default_input_config().map_err(|error| {
        RuntimeError::Audio(format!("cannot read microphone configuration: {error}"))
    })?;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let channels = config.channels;
    let sample_rate = config.sample_rate.0;
    let samples = Arc::new(Mutex::new(Vec::<i16>::new()));
    let stream_error = Arc::new(Mutex::new(None::<String>));

    let stream = match sample_format {
        cpal::SampleFormat::F32 => build_input_stream_f32(
            &device,
            &config,
            Arc::clone(&samples),
            Arc::clone(&stream_error),
        )?,
        cpal::SampleFormat::I16 => build_input_stream_i16(
            &device,
            &config,
            Arc::clone(&samples),
            Arc::clone(&stream_error),
        )?,
        cpal::SampleFormat::U16 => build_input_stream_u16(
            &device,
            &config,
            Arc::clone(&samples),
            Arc::clone(&stream_error),
        )?,
        format => {
            return Err(RuntimeError::Audio(format!(
                "unsupported microphone sample format: {format:?}"
            )))
        }
    };

    stream.play().map_err(|error| {
        RuntimeError::Audio(format!("cannot start microphone capture: {error}"))
    })?;
    if let Some(seconds) = duration {
        eprintln!("Recording microphone for {seconds} seconds...");
        thread::sleep(Duration::from_secs(seconds));
    } else {
        eprintln!("Recording microphone. Press Enter to stop.");
        let mut line = String::new();
        io::stdin().read_line(&mut line).map_err(|error| {
            RuntimeError::Audio(format!("cannot read recording stop input: {error}"))
        })?;
    }
    drop(stream);

    if let Some(error) = stream_error.lock().unwrap().clone() {
        return Err(RuntimeError::Audio(error));
    }
    let samples = samples.lock().unwrap().clone();
    if samples.is_empty() {
        return Err(RuntimeError::Audio(
            "microphone returned no samples".to_string(),
        ));
    }
    let wav_bytes = encode_wav(&samples, sample_rate, channels)?;
    AudioInput::from_wav_bytes("microphone.wav", wav_bytes)
}

fn build_input_stream_f32(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: Arc<Mutex<Vec<i16>>>,
    stream_error: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, RuntimeError> {
    device
        .build_input_stream(
            config,
            move |data: &[f32], _| append_f32(data, &samples),
            move |error| set_stream_error(&stream_error, error.to_string()),
            None,
        )
        .map_err(|error| RuntimeError::Audio(format!("cannot open microphone stream: {error}")))
}

fn build_input_stream_i16(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: Arc<Mutex<Vec<i16>>>,
    stream_error: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, RuntimeError> {
    device
        .build_input_stream(
            config,
            move |data: &[i16], _| append_i16(data, &samples),
            move |error| set_stream_error(&stream_error, error.to_string()),
            None,
        )
        .map_err(|error| RuntimeError::Audio(format!("cannot open microphone stream: {error}")))
}

fn build_input_stream_u16(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: Arc<Mutex<Vec<i16>>>,
    stream_error: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, RuntimeError> {
    device
        .build_input_stream(
            config,
            move |data: &[u16], _| append_u16(data, &samples),
            move |error| set_stream_error(&stream_error, error.to_string()),
            None,
        )
        .map_err(|error| RuntimeError::Audio(format!("cannot open microphone stream: {error}")))
}

fn append_f32(data: &[f32], samples: &Arc<Mutex<Vec<i16>>>) {
    if let Ok(mut output) = samples.lock() {
        output.extend(
            data.iter()
                .map(|sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16),
        );
    }
}

fn append_i16(data: &[i16], samples: &Arc<Mutex<Vec<i16>>>) {
    if let Ok(mut output) = samples.lock() {
        output.extend_from_slice(data);
    }
}

fn append_u16(data: &[u16], samples: &Arc<Mutex<Vec<i16>>>) {
    if let Ok(mut output) = samples.lock() {
        output.extend(data.iter().map(|sample| (*sample as i32 - 32768) as i16));
    }
}

fn set_stream_error(target: &Arc<Mutex<Option<String>>>, message: String) {
    if let Ok(mut error) = target.lock() {
        *error = Some(message);
    }
}

fn encode_wav(samples: &[i16], sample_rate: u32, channels: u16) -> Result<Vec<u8>, RuntimeError> {
    if sample_rate == 0 || channels == 0 || !samples.len().is_multiple_of(channels as usize) {
        return Err(RuntimeError::Audio(
            "microphone samples do not match the channel count".to_string(),
        ));
    }

    let data_size = samples.len().checked_mul(2).ok_or_else(|| {
        RuntimeError::Audio("recording is too large to encode as WAV".to_string())
    })?;
    let riff_size = 36usize.checked_add(data_size).ok_or_else(|| {
        RuntimeError::Audio("recording is too large to encode as WAV".to_string())
    })?;
    if riff_size > u32::MAX as usize {
        return Err(RuntimeError::Audio(
            "WAV exceeds RIFF's 32-bit size limit".into(),
        ));
    }
    let byte_rate = sample_rate
        .checked_mul(channels as u32)
        .and_then(|value| value.checked_mul(2))
        .ok_or_else(|| RuntimeError::Audio("invalid microphone format".to_string()))?;
    let block_align = channels
        .checked_mul(2)
        .ok_or_else(|| RuntimeError::Audio("invalid microphone channel count".to_string()))?;

    let mut wav = Vec::with_capacity(44 + data_size);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(riff_size as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data_size as u32).to_le_bytes());
    for sample in samples {
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    Ok(wav)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub text: String,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteSummary {
    pub topic: String,
    pub summary: String,
    #[serde(rename = "keyPoints")]
    pub key_points: Vec<String>,
    #[serde(rename = "actionItems")]
    pub action_items: Vec<String>,
    pub uncertainties: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioNoteResult {
    pub transcript: Transcript,
    pub summary: NoteSummary,
}

#[derive(Debug, Clone)]
pub struct LmStudioClient {
    client: Client,
    endpoint: String,
    token: Option<String>,
}

impl LmStudioClient {
    pub fn complete_message(&self, payload: &Value) -> Result<Value, RuntimeError> {
        let mut request = self
            .client
            .post(format!("{}/v1/chat/completions", self.endpoint))
            .json(payload);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let body: Value = request.send()?.error_for_status()?.json()?;
        body.pointer("/choices/0/message")
            .cloned()
            .ok_or_else(|| RuntimeError::Model("missing assistant message".into()))
    }
    pub fn from_environment() -> Result<Self, RuntimeError> {
        let endpoint =
            std::env::var("CAMEL_LM_ENDPOINT").unwrap_or_else(|_| DEFAULT_ENDPOINT.to_string());
        let timeout_seconds = std::env::var("CAMEL_LM_TIMEOUT_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(300);
        let token = std::env::var("CAMEL_LM_TOKEN")
            .ok()
            .or_else(|| std::env::var("LM_API_TOKEN").ok())
            .filter(|value| !value.trim().is_empty());

        Self::with_endpoint(&endpoint, token, Duration::from_secs(timeout_seconds))
    }

    pub fn with_endpoint(
        endpoint: &str,
        token: Option<String>,
        timeout: Duration,
    ) -> Result<Self, RuntimeError> {
        if timeout.is_zero() {
            return Err(RuntimeError::InvalidInput(
                "request timeout must be positive".into(),
            ));
        }
        let client = Client::builder().timeout(timeout).build()?;
        let endpoint = endpoint.trim_end_matches('/');
        let endpoint = endpoint.strip_suffix("/v1").unwrap_or(endpoint);
        Ok(Self {
            client,
            endpoint: endpoint.to_string(),
            token,
        })
    }

    pub fn transcribe(&self, model: &str, audio: &AudioInput) -> Result<Transcript, RuntimeError> {
        let content = vec![
            json!({
                "type": "text",
                "text": "Perform automatic speech recognition. Transcribe this audio faithfully. Return only the spoken transcript."
            }),
            audio.content_part(),
        ];
        let response = self.chat(
            model,
            json!({
                "role": "user",
                "content": content
            }),
            0.0,
        )?;
        Ok(Transcript {
            text: response,
            language: "unknown".to_string(),
        })
    }

    pub fn summarize(
        &self,
        model: &str,
        topic: &str,
        transcript: &Transcript,
    ) -> Result<NoteSummary, RuntimeError> {
        let prompt = format!(
            "Requested topic: {topic}\n\nTranscript:\n{}\n\nReturn only JSON with keys topic, summary, keyPoints, actionItems, and uncertainties. Prioritize confirmed information relevant to the requested topic. Do not invent facts.",
            transcript.text
        );
        let response = self.chat(
            model,
            json!({
                "role": "user",
                "content": prompt
            }),
            0.2,
        )?;
        parse_summary(&response, topic)
    }

    fn chat(&self, model: &str, message: Value, temperature: f64) -> Result<String, RuntimeError> {
        if model.trim().is_empty() {
            return Err(RuntimeError::InvalidInput(
                "model name cannot be empty".to_string(),
            ));
        }

        let url = format!("{}/v1/chat/completions", self.endpoint);
        let mut request = self
            .client
            .post(url)
            .json(&chat_payload(model, message, temperature));
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }

        let response = request.send()?.error_for_status()?;
        let body: Value = response.json()?;
        extract_content(&body)
    }
}

fn chat_payload(model: &str, message: Value, temperature: f64) -> Value {
    json!({
        "model": model,
        "messages": [message],
        "temperature": temperature,
        "stream": false
    })
}

fn extract_content(body: &Value) -> Result<String, RuntimeError> {
    let content = body.pointer("/choices/0/message/content").ok_or_else(|| {
        RuntimeError::Model(format!("missing choices[0].message.content: {body}"))
    })?;
    if let Some(text) = content.as_str() {
        return Ok(text.to_string());
    }
    if let Some(parts) = content.as_array() {
        let joined = parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("");
        if !joined.is_empty() {
            return Ok(joined);
        }
    }
    Err(RuntimeError::Model(format!(
        "unsupported message content: {content}"
    )))
}

fn parse_summary(response: &str, topic: &str) -> Result<NoteSummary, RuntimeError> {
    let candidate = response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    match serde_json::from_str(candidate) {
        Ok(summary) => Ok(summary),
        Err(_) => Ok(NoteSummary {
            topic: topic.to_string(),
            summary: response.trim().to_string(),
            key_points: Vec::new(),
            action_items: Vec::new(),
            uncertainties: vec![
                "The summarizer did not return the requested JSON shape.".to_string()
            ],
        }),
    }
}

pub fn validate_audio_script(source: &str) -> Result<(), RuntimeError> {
    language::CompiledProgram::from_source("audio.camel", source)
        .map(|_| ())
        .map_err(|e| RuntimeError::InvalidInput(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn audio_input_resolves_relative_path_and_encodes_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let mut file = fs::File::create(directory.path().join("note.wav")).unwrap();
        file.write_all(b"RIFF-test-audio").unwrap();

        let audio = AudioInput::from_path(directory.path(), "note.wav").unwrap();

        assert_eq!(audio.format, "wav");
        assert_eq!(audio.data_base64, BASE64.encode(b"RIFF-test-audio"));
    }

    #[test]
    fn unsupported_audio_extensions_fail_before_network_access() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("note.txt"), b"not audio").unwrap();

        let error = AudioInput::from_path(directory.path(), "note.txt").unwrap_err();

        assert!(error.to_string().contains("unsupported audio extension"));
    }

    #[test]
    fn summary_json_is_parsed_into_typed_fields() {
        let summary = parse_summary(
            r#"{"topic":"launch","summary":"Ship Friday.","keyPoints":["Friday"],"actionItems":["Deploy"],"uncertainties":[]}"#,
            "general",
        )
        .unwrap();

        assert_eq!(summary.topic, "launch");
        assert_eq!(summary.action_items, vec!["Deploy"]);
    }

    #[test]
    fn invalid_summary_json_falls_back_without_losing_text() {
        let summary = parse_summary("Ship Friday.", "launch").unwrap();

        assert_eq!(summary.topic, "launch");
        assert_eq!(summary.summary, "Ship Friday.");
        assert_eq!(summary.uncertainties.len(), 1);
    }

    #[test]
    fn audio_payload_uses_openai_input_audio_content() {
        let audio = AudioInput {
            path: PathBuf::from("note.wav"),
            format: "wav".to_string(),
            data_base64: "encoded-audio".to_string(),
        };
        let payload = chat_payload(
            "LiquidAI/LFM2.5-Audio-1.5B-GGUF",
            json!({
                "role": "user",
                "content": [
                    {"type": "text", "text": "Perform automatic speech recognition."},
                    audio.content_part()
                ]
            }),
            0.0,
        );

        assert_eq!(payload["model"], "LiquidAI/LFM2.5-Audio-1.5B-GGUF");
        assert_eq!(payload["messages"][0]["content"][1]["type"], "input_audio");
        assert_eq!(
            payload["messages"][0]["content"][1]["input_audio"]["format"],
            "wav"
        );
        assert_eq!(
            payload["messages"][0]["content"][1]["input_audio"]["data"],
            "encoded-audio"
        );
    }

    #[test]
    fn microphone_samples_encode_as_pcm_wav() {
        let wav = encode_wav(&[0, 1024, -1024, 0], 16_000, 1).unwrap();

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 8);
        assert_eq!(wav.len(), 52);
    }
}
