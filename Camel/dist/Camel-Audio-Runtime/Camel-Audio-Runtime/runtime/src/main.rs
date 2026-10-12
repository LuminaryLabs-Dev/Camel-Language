use camel_audio_runtime::{
    validate_audio_script, AudioInput, AudioNoteResult, LmStudioClient, RuntimeError,
};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

struct Options {
    source: PathBuf,
    audio_path: Option<String>,
    topic: String,
    json: bool,
    trace: bool,
    check: bool,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Camel error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), RuntimeError> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }
    let options = parse_args(args)?;
    let source = fs::read_to_string(&options.source)?;
    validate_audio_script(&source)?;

    if options.check {
        println!("ok: audio note workflow validated");
        return Ok(());
    }

    let audio_path = options.audio_path.ok_or_else(|| {
        RuntimeError::InvalidInput(
            "provide an audio path and optional topic after '--'".to_string(),
        )
    })?;
    let source_dir = options.source.parent().unwrap_or_else(|| Path::new("."));
    let audio = AudioInput::from_path(source_dir, &audio_path)?;
    let client = LmStudioClient::from_environment()?;
    let audio_model = env::var("CAMEL_AUDIO_MODEL")
        .unwrap_or_else(|_| "LiquidAI/LFM2.5-Audio-1.5B-GGUF".to_string());
    let summary_model = env::var("CAMEL_SUMMARY_MODEL")
        .unwrap_or_else(|_| "LiquidAI/LFM2.5-1.2B-Thinking".to_string());

    if options.trace {
        eprintln!("audio: {}", audio.path.display());
        eprintln!("transcription model: {audio_model}");
        eprintln!("summary model: {summary_model}");
    }

    let transcript = client.transcribe(&audio_model, &audio)?;
    let summary = client.summarize(&summary_model, &options.topic, &transcript)?;
    let result = AudioNoteResult {
        transcript,
        summary,
    };

    if options.json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("TRANSCRIPT\n{}", result.transcript.text);
        println!("\nSUMMARY\n{}", result.summary.summary);
        println!("\nKEY POINTS");
        for point in &result.summary.key_points {
            println!("- {point}");
        }
        println!("\nACTION ITEMS");
        for item in &result.summary.action_items {
            println!("- {item}");
        }
    }
    Ok(())
}

fn parse_args(args: Vec<String>) -> Result<Options, RuntimeError> {
    let check = args.first().map(String::as_str) == Some("check");
    let source_index = if check { 1 } else { 0 };
    let source = args
        .get(source_index)
        .ok_or_else(|| RuntimeError::InvalidInput("missing .camel source path".to_string()))?;
    let mut json_output = false;
    let mut trace = false;
    let mut separator = None;

    for (index, arg) in args.iter().enumerate().skip(source_index + 1) {
        match arg.as_str() {
            "--json" => json_output = true,
            "--trace" => trace = true,
            "--lm" => {}
            "--" => {
                separator = Some(index);
                break;
            }
            value if value.starts_with('-') => {
                return Err(RuntimeError::InvalidInput(format!(
                    "unknown option: {value}"
                )))
            }
            _ => {}
        }
    }

    let workflow_args = separator
        .map(|index| args[index + 1..].to_vec())
        .unwrap_or_default();
    let audio_path = workflow_args.first().cloned();
    let topic = workflow_args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "general".to_string());

    Ok(Options {
        source: PathBuf::from(source),
        audio_path,
        topic,
        json: json_output,
        trace,
        check,
    })
}

fn print_help() {
    println!(
        "{}",
        r#"Camel audio runtime

Usage:
  camel-audio-runtime check <file.camel>
  camel-audio-runtime <file.camel> [--lm] [--json] [--trace] -- <audio-path> [topic]

Environment:
  CAMEL_LM_ENDPOINT        LM Studio server (default: http://localhost:1234)
  CAMEL_LM_TOKEN           Optional API token
  CAMEL_AUDIO_MODEL        Transcription model override
  CAMEL_SUMMARY_MODEL      Summary model override
  CAMEL_LM_TIMEOUT_SECONDS Request timeout (default: 300)"#
    );
}
