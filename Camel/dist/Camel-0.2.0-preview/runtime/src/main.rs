use camel_audio_runtime::{
    language::{CompiledProgram, Engine, Host, Limits, Type, Value},
    LmStudioClient,
};
use std::{env, path::PathBuf, sync::Arc};

fn main() {
    if let Err(e) = run() {
        eprintln!("Camel error: {e}");
        std::process::exit(1);
    }
}
struct Options {
    source: PathBuf,
    entry: String,
    check: bool,
    json: bool,
    trace: bool,
    args: Vec<String>,
    seconds: Option<Option<u64>>,
    save: Option<PathBuf>,
    steps: usize,
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args.first().is_some_and(|s| s == "--help" || s == "-h") {
        help();
        return Ok(());
    }
    if args == ["--version"] {
        println!("Camel 0.2.0-preview");
        return Ok(());
    }
    let options = options(args).map_err(std::io::Error::other)?;
    let program = CompiledProgram::load(&options.source)?;
    if options.check {
        println!(
            "ok: {} (parsed and name-checked; no program execution)",
            options.source.display()
        );
        return Ok(());
    }
    let params = program.entry_parameters(&options.entry)?;
    if options.args.len() > params.len() {
        return Err("too many workflow arguments".into());
    }
    let args = options
        .args
        .iter()
        .zip(&params)
        .map(|(raw, (_, ty))| argument(raw, ty))
        .collect::<Result<Vec<_>, _>>()?;
    let host = Host {
        provider: Some(Arc::new(LmStudioClient::from_environment()?)),
        recording_seconds: options.seconds,
        save_recording: options.save,
        ..Host::default()
    };
    if options.trace {
        eprintln!(
            "executing workflow {} from {} (step budget {})",
            options.entry,
            options.source.display(),
            options.steps
        );
    }
    let result = Engine::new(program, host)
        .with_limits(Limits {
            steps: options.steps,
            ..Limits::default()
        })
        .run(&options.entry, args)?;
    if options.json {
        let site = camel_audio_runtime::language::Site {
            file: options.source.display().to_string(),
            line: 1,
            column: 1,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"value":result.value.to_json(&site)?,"logs":result.logs})
            )?
        );
    } else {
        for log in result.logs {
            println!("{log}");
        }
        if !matches!(result.value, Value::Null) {
            println!("{}", result.value.display());
        }
    }
    Ok(())
}
fn argument(raw: &str, ty: &Type) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(match ty {
        Type::Named(n) if n == "string" => Value::String(raw.into()),
        Type::Named(n) if n == "int" => Value::Int(raw.parse()?),
        Type::Named(n) if n == "double" => Value::Float(raw.parse()?),
        Type::Named(n) if n == "bool" => Value::Bool(raw.parse()?),
        Type::Task(_) => return Err("tasks cannot be CLI arguments".into()),
        _ => Value::from_json(serde_json::from_str(raw)?),
    })
}
fn options(args: Vec<String>) -> Result<Options, String> {
    let command = args.first().map(String::as_str).unwrap_or("");
    let start = if ["run", "check", "record"].contains(&command) {
        1
    } else {
        0
    };
    let source = args.get(start).ok_or("missing .camel source path")?;
    let mut o = Options {
        source: source.into(),
        entry: "Main".into(),
        check: command == "check",
        json: false,
        trace: false,
        args: Vec::new(),
        seconds: if command == "record" {
            Some(None)
        } else {
            None
        },
        save: None,
        steps: 100_000,
    };
    let mut i = start + 1;
    while i < args.len() {
        match args[i].as_str() {
            "--" => {
                o.args = args[i + 1..].to_vec();
                break;
            }
            "--json" => o.json = true,
            "--trace" => o.trace = true,
            "--lm" => {}
            "--until-enter" => o.seconds = Some(None),
            "--workflow" | "--steps" | "--seconds" | "--save-recording" => {
                let flag = &args[i];
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| format!("{flag} requires a value"))?;
                match flag.as_str() {
                    "--workflow" => o.entry = value.clone(),
                    "--steps" => {
                        o.steps = value
                            .parse()
                            .map_err(|_| "--steps must be a positive integer")?;
                        if o.steps == 0 {
                            return Err("--steps must be positive".into());
                        }
                    }
                    "--seconds" => {
                        let seconds: u64 =
                            value.parse().map_err(|_| "--seconds must be an integer")?;
                        if !(1..=3600).contains(&seconds) {
                            return Err("--seconds must be 1..3600".into());
                        }
                        o.seconds = Some(Some(seconds));
                    }
                    "--save-recording" => o.save = Some(PathBuf::from(value)),
                    _ => unreachable!(),
                }
            }
            other => {
                return Err(format!(
                    "unknown option '{other}'; workflow arguments belong after --"
                ))
            }
        }
        i += 1;
    }
    Ok(o)
}
fn help() {
    println!(
        r#"Camel 0.2.0-preview — source-backed agent programming language

  camel check <file.camel>
  camel [run] <file.camel> [--workflow Main] [--json] [--trace] [--steps N] -- [arguments]
  camel record <file.camel> [--seconds N|--until-enter] [--save-recording path] -- [arguments]

record is a compatibility command that overrides record(seconds) builtins.
Without it, the program controls when and how long to record. Nothing records during check.
--json emits one object containing the workflow value and captured logs.
Model calls use CAMEL_LM_ENDPOINT (default http://localhost:1234), CAMEL_LM_TOKEN,
and CAMEL_LM_TIMEOUT_SECONDS (default 300). Models come from agent declarations.
The camel-audio-runtime executable is an alias for this same interpreter."#
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn check_does_not_require_audio_arguments() {
        let o = options(vec!["check".into(), "notes.camel".into()]).unwrap();
        assert!(o.check);
        assert!(o.args.is_empty());
    }
    #[test]
    fn arguments_after_separator_remain_literal() {
        let o = options(vec!["a.camel".into(), "--".into(), "--help".into()]).unwrap();
        assert_eq!(o.args, vec!["--help"]);
    }
    #[test]
    fn stray_arguments_are_rejected() {
        assert!(options(vec!["a.camel".into(), "topic".into()]).is_err());
    }
}
