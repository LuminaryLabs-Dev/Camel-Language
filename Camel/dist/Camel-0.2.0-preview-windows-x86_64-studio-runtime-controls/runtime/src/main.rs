use camel_audio_runtime::{
    language::{analyze_file, Engine, Host, Limits, Value},
    libraries, LmStudioClient,
};
use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

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
    tool_calls: usize,
    tokens: usize,
    timeout_seconds: u64,
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "library" || s == "lib") {
        return library_command(&args[1..]);
    }
    if args.first().is_some_and(|s| s == "--gui") {
        return launch_studio(&args[1..]);
    }
    if args.is_empty() || args.first().is_some_and(|s| s == "--help" || s == "-h") {
        help();
        return Ok(());
    }
    if args == ["--version"] {
        println!("Camel 0.2.0-preview");
        return Ok(());
    }
    let options = options(args).map_err(std::io::Error::other)?;
    let analysis = analyze_file(&options.source);
    if !analysis.is_valid() {
        for diagnostic in analysis.diagnostics() {
            eprintln!("{diagnostic}");
        }
        return Err(format!(
            "analysis failed with {} diagnostic(s)",
            analysis.diagnostics().len()
        )
        .into());
    }
    for diagnostic in analysis.diagnostics() {
        eprintln!("{diagnostic}");
    }
    let program = analysis.into_program()?;
    if options.check {
        println!(
            "ok: {} (Camel {}; parsed, checked, linted; no program execution)",
            options.source.display(),
            program.language_version()
        );
        return Ok(());
    }
    let args = program.parse_arguments(&options.entry, &options.args)?;
    let host = Host {
        provider: Some(Arc::new(LmStudioClient::from_environment()?)),
        recording_seconds: options.seconds,
        save_recording: options.save,
        ..Host::default()
    };
    if options.trace {
        eprintln!(
            "executing workflow {} from {} (step {}, tool-call {}, token {}, time {}s budgets)",
            options.entry,
            options.source.display(),
            options.steps,
            options.tool_calls,
            options.tokens,
            options.timeout_seconds
        );
    }
    let result = Engine::new(program, host)
        .with_limits(Limits {
            steps: options.steps,
            tool_calls: options.tool_calls,
            tokens: options.tokens,
            deadline: std::time::Duration::from_secs(options.timeout_seconds),
            ..Limits::default()
        })
        .run(&options.entry, args)?;
    if options.json {
        let site = camel_audio_runtime::language::Site {
            file: options.source.display().to_string(),
            line: 1,
            column: 1,
            offset: 0,
            end_line: 1,
            end_column: 1,
            end_offset: 0,
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
fn launch_studio(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let studio_name = format!("camel-studio{}", env::consts::EXE_SUFFIX);
    let executable = if let Some(path) = env::var_os("CAMEL_STUDIO_PATH").map(PathBuf::from) {
        if !path.is_file() {
            return Err(format!(
                "CAMEL_STUDIO_PATH does not point to a file: {}",
                path.display()
            )
            .into());
        }
        path
    } else {
        let current_exe = env::current_exe()?;
        let sibling = current_exe.with_file_name(&studio_name);
        if sibling.is_file() {
            sibling
        } else {
            let preferred_profile = current_exe
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str());
            let profiles = if preferred_profile == Some("release") {
                ["release", "debug"]
            } else {
                ["debug", "release"]
            };
            let repo_candidate = current_exe.ancestors().find_map(|ancestor| {
                if !ancestor.join("studio/Cargo.toml").is_file() {
                    return None;
                }
                profiles
                    .into_iter()
                    .map(|profile| {
                        ancestor
                            .join("studio/target")
                            .join(profile)
                            .join(&studio_name)
                    })
                    .find(|candidate| candidate.is_file())
            });
            match repo_candidate {
                Some(path) => path,
                None => {
                    if let Some(path) = studio_on_path() {
                        path
                    } else {
                        return Err(format!(
                            "Camel Studio was not found; build/install {studio_name} beside camel, add it to PATH, or set CAMEL_STUDIO_PATH"
                        )
                        .into());
                    }
                }
            }
        }
    };
    Command::new(executable).args(args).spawn()?;
    Ok(())
}
fn studio_on_path() -> Option<PathBuf> {
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|directory| directory.join(format!("camel-studio{}", env::consts::EXE_SUFFIX)))
            .find(|candidate| candidate.is_file())
    })
}
fn library_command(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut positional = Vec::new();
    let mut source = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--catalog" {
            if source.is_some() {
                return Err("--catalog may only appear once".into());
            }
            source = Some(
                args.next()
                    .ok_or("--catalog requires a file path or URL")?
                    .as_str(),
            );
        } else {
            positional.push(arg.as_str());
        }
    }
    let command = positional.first().copied().unwrap_or("list");
    let argument = positional.get(1).copied();
    if positional.len() > 2 {
        return Err("too many library arguments".into());
    }
    match command {
        "help" | "--help" | "-h" => println!("camel library [list|search <query>|install <name>|update <name>|remove <name>] [--catalog <path-or-url>]"),
        "remove" | "uninstall" => {
            let name = argument.ok_or("usage: camel library remove <name>")?;
            println!("{} {name}", if libraries::remove(name)? { "removed" } else { "already absent:" });
        }
        "list" | "search" => {
            if command == "list" && argument.is_some() { return Err("usage: camel library list".into()); }
            let query = if command == "search" { argument.ok_or("usage: camel library search <query>")? } else { "" }.to_lowercase();
            for item in libraries::catalog(source)?.libraries.iter().filter(|item| item.name.to_lowercase().contains(&query) || item.description.to_lowercase().contains(&query)) {
                let state = if libraries::installed_library(&item.name).is_some() { "installed" } else { "available" };
                println!("{}\t{}\t{}\t{}", item.name, item.version, state, item.description);
            }
        }
        "install" | "update" => {
            let name = argument.ok_or("usage: camel library install <name>")?;
            let catalog = if command == "update" && source.is_none() {
                libraries::catalog(Some(libraries::CATALOG_URL))?
            } else {
                libraries::catalog(source)?
            };
            let item = catalog.libraries.iter().find(|item| item.name == name).ok_or_else(|| format!("library '{name}' is not in the catalog"))?;
            println!("installed {} {} at {}", item.name, item.version, libraries::install(item)?.display());
        }
        _ => return Err(format!("unknown library command '{command}'; use camel library help").into()),
    }
    Ok(())
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
        tool_calls: 256,
        tokens: 32_768,
        timeout_seconds: 600,
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
            "--workflow" | "--steps" | "--tool-calls" | "--tokens" | "--timeout-seconds"
            | "--seconds" | "--save-recording" => {
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
                    "--tool-calls" => {
                        o.tool_calls = value
                            .parse()
                            .map_err(|_| "--tool-calls must be a positive integer")?;
                        if o.tool_calls == 0 {
                            return Err("--tool-calls must be positive".into());
                        }
                    }
                    "--tokens" => {
                        o.tokens = value
                            .parse()
                            .map_err(|_| "--tokens must be a positive integer")?;
                        if o.tokens == 0 {
                            return Err("--tokens must be positive".into());
                        }
                    }
                    "--timeout-seconds" => {
                        o.timeout_seconds = value
                            .parse()
                            .map_err(|_| "--timeout-seconds must be 1..86400")?;
                        if !(1..=86_400).contains(&o.timeout_seconds) {
                            return Err("--timeout-seconds must be 1..86400".into());
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
  camel [run] <file.camel> [--workflow Main] [--json] [--trace] [--steps N] [--tool-calls N] [--tokens N] [--timeout-seconds N] -- [arguments]
  camel record <file.camel> [--seconds N|--until-enter] [--save-recording path] -- [arguments]
  camel --gui [file.camel]
  camel library [list|search <query>|install <name>|update <name>|remove <name>] [--catalog <path-or-url>]

record is a compatibility command that overrides record(seconds) host input.
Without it, the program controls when and how long to record. Nothing records during check.
--json emits one object containing the workflow value and captured logs.
Model calls use CAMEL_LM_ENDPOINT (default http://localhost:1234), CAMEL_LM_TOKEN,
and CAMEL_LM_TIMEOUT_SECONDS (default 300). Models come from agent declarations.
--gui launches Camel Studio; set CAMEL_STUDIO_PATH to select its executable.
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
    #[test]
    fn run_budgets_are_configurable() {
        let parsed = options(vec![
            "a.camel".into(),
            "--tool-calls".into(),
            "12".into(),
            "--tokens".into(),
            "2048".into(),
            "--timeout-seconds".into(),
            "30".into(),
        ])
        .unwrap();
        assert_eq!(parsed.tool_calls, 12);
        assert_eq!(parsed.tokens, 2048);
        assert_eq!(parsed.timeout_seconds, 30);
        assert!(options(vec![
            "a.camel".into(),
            "--timeout-seconds".into(),
            "0".into()
        ])
        .is_err());
    }
}
