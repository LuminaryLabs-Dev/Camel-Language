use camel_audio_runtime::{
    language::{analyze_file, Capability, Engine, Host, Limits, ReplayCassette, Value},
    libraries, LmStudioClient,
};
use std::{
    collections::BTreeSet,
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
    approvals: BTreeSet<Capability>,
    checkpoint: Option<PathBuf>,
    resume: Option<PathBuf>,
    record_cassette: Option<PathBuf>,
    replay_cassette: Option<PathBuf>,
    evaluation_report: Option<PathBuf>,
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "library" || s == "lib") {
        return library_command(&args[1..]);
    }
    if args.first().is_some_and(|s| s == "--gui") {
        return launch_studio(&args[1..]);
    }
    if args.first().is_some_and(|s| s == "--update") {
        return launch_local_update(&args[1..]);
    }
    if args.is_empty() || args.first().is_some_and(|s| s == "--help" || s == "-h") {
        help();
        return Ok(());
    }
    if args == ["--version"] {
        println!("Camel v.0.3.0");
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
    if options.record_cassette.is_some() && options.replay_cassette.is_some() {
        return Err("--record-cassette and --replay-cassette are mutually exclusive".into());
    }
    if options.evaluation_report.is_some() && options.replay_cassette.is_none() {
        return Err(
            "--evaluation-report requires --replay-cassette for deterministic output".into(),
        );
    }
    let cassette = if let Some(path) = &options.record_cassette {
        let cassette = if options.resume.is_some() {
            ReplayCassette::continue_recording(path)
        } else {
            ReplayCassette::record_to(path)
        };
        Some(cassette.map_err(std::io::Error::other)?)
    } else if let Some(path) = &options.replay_cassette {
        Some(ReplayCassette::replay_from(path).map_err(std::io::Error::other)?)
    } else {
        None
    };
    let host = Host {
        provider: if options.replay_cassette.is_some() {
            None
        } else {
            Some(Arc::new(LmStudioClient::from_environment()?))
        },
        recording_seconds: options.seconds,
        save_recording: options.save,
        approved_capabilities: Some(options.approvals.clone()),
        cassette,
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
    let mut engine = Engine::new(program, host).with_limits(Limits {
        steps: options.steps,
        tool_calls: options.tool_calls,
        tokens: options.tokens,
        deadline: std::time::Duration::from_secs(options.timeout_seconds),
        ..Limits::default()
    });
    if let Some(path) = &options.checkpoint {
        engine = engine.with_checkpoint_file(path);
    }
    if let Some(path) = &options.resume {
        engine = engine.resume_from(path);
    }
    let result = engine.run(&options.entry, args)?;
    let site = camel_audio_runtime::language::Site {
        file: options.source.display().to_string(),
        line: 1,
        column: 1,
        offset: 0,
        end_line: 1,
        end_column: 1,
        end_offset: 0,
    };
    if let Some(path) = &options.evaluation_report {
        let report = serde_json::json!({
            "format": "camel-evaluation-v1",
            "sourceFingerprint": result.source_fingerprint,
            "cassetteFingerprint": result.cassette_fingerprint,
            "workflow": options.entry,
            "result": result.value.to_json(&site)?,
            "logs": result.logs,
            "usage": {"tokens": result.tokens_used, "toolCalls": result.tool_calls_used}
        });
        write_json_atomic(path, &report)?;
    }
    if options.json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"value":result.value.to_json(&site)?,"logs":result.logs,"usage":{"tokens":result.tokens_used,"toolCalls":result.tool_calls_used},"sourceFingerprint":result.source_fingerprint,"cassetteFingerprint":result.cassette_fingerprint})
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

fn launch_local_update(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 1 {
        return Err("Usage: camel --update [extracted-distribution-folder]".into());
    }
    let bundle = match args.first() {
        Some(path) => PathBuf::from(path).canonicalize()?,
        None => env::current_dir()?.canonicalize()?,
    };
    let setup_name = if cfg!(windows) {
        "Camel-Setup.exe"
    } else {
        "camel-setup"
    };
    let setup = bundle.join(setup_name);
    if !bundle.join("INSTALLER_BUNDLE.json").is_file() || !setup.is_file() {
        return Err("The folder must contain an extracted Camel distribution, including INSTALLER_BUNDLE.json and the setup program.".into());
    }

    let executable = env::current_exe()?.canonicalize()?;
    let working_directory = env::current_dir().ok();
    let installation = find_installation(&executable, working_directory.as_deref()).ok_or(
        "Could not locate an installed Camel folder. Run this from an installed Camel command or keep its bin folder on PATH.",
    )?;

    Command::new(setup)
        .args([
            "--bundle",
            bundle
                .to_str()
                .ok_or("Invalid distribution path encoding.")?,
        ])
        .args([
            "--destination",
            installation
                .to_str()
                .ok_or("Invalid installation path encoding.")?,
        ])
        .args(["--wait-for-pid", &std::process::id().to_string()])
        .spawn()?;
    println!(
        "Setup opened from the local update bundle. Review the destination and install when ready."
    );
    Ok(())
}

fn find_installation(executable: &Path, working_directory: Option<&Path>) -> Option<PathBuf> {
    let installation_at = |root: &Path| {
        let root = root.canonicalize().ok()?;
        let marker = serde_json::from_slice::<serde_json::Value>(
            &std::fs::read(root.join("INSTALLATION.json")).ok()?,
        )
        .ok()?;
        let recorded = Path::new(marker.get("destination")?.as_str()?)
            .canonicalize()
            .ok()?;
        (marker.get("product")?.as_str()? == "Camel" && recorded == root).then_some(root)
    };
    let installation_for = |candidate: &Path| {
        let candidate = candidate.canonicalize().ok()?;
        installation_at(candidate.parent()?.parent()?)
    };

    if let Some(working_directory) = working_directory {
        if let Some(installation) = installation_at(working_directory) {
            return Some(installation);
        }
    }
    if let Some(installation) = installation_for(executable) {
        return Some(installation);
    }
    let camel_name = format!("camel{}", env::consts::EXE_SUFFIX);
    if let Some(paths) = env::var_os("PATH") {
        for directory in env::split_paths(&paths) {
            let candidate = directory.join(&camel_name);
            if candidate.canonicalize().ok().as_deref() != Some(executable) {
                if let Some(installation) = installation_for(&candidate) {
                    return Some(installation);
                }
            }
        }
    }
    None
}

fn studio_on_path() -> Option<PathBuf> {
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|directory| directory.join(format!("camel-studio{}", env::consts::EXE_SUFFIX)))
            .find(|candidate| candidate.is_file())
    })
}
fn write_json_atomic(
    path: &Path,
    value: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut pending = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut pending, value)?;
    pending.as_file().sync_all()?;
    pending.persist(path).map_err(|error| error.error)?;
    Ok(())
}
fn library_command(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut positional = Vec::new();
    let mut source = None;
    let mut output = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--catalog" => {
                if source.is_some() {
                    return Err("--catalog may only appear once".into());
                }
                source = Some(
                    args.next()
                        .ok_or("--catalog requires a file path or URL")?
                        .as_str(),
                );
            }
            "--output" => {
                if output.is_some() {
                    return Err("--output may only appear once".into());
                }
                output = Some(PathBuf::from(
                    args.next().ok_or("--output requires a path")?,
                ));
            }
            _ => positional.push(arg.as_str()),
        }
    }
    let command = positional.first().copied().unwrap_or("list");
    let argument = positional.get(1).copied();
    if positional.len() > 2 && positional.first().copied() != Some("lock") {
        return Err("too many library arguments".into());
    }
    match command {
        "help" | "--help" | "-h" => println!("camel library [list|search <query>|install <name>|update <name>|remove <name>|lock <name>|verify-lock [path]|install-locked [path]] [--catalog <path-or-url>] [--output <path>]"),
        "lock" => {
            if positional.len() < 2 { return Err("usage: camel library lock <name>... [--output camel.lock]".into()); }
            let names = positional[1..].iter().map(|name| (*name).to_owned()).collect::<Vec<_>>();
            let lock = libraries::create_lock(&libraries::catalog(source)?, &names)?;
            let path = output.unwrap_or_else(|| PathBuf::from("camel.lock"));
            libraries::write_lock(&path, &lock)?;
            println!("locked {} package(s) to {}", lock.libraries.len(), path.display());
        }
        "verify-lock" => {
            if output.is_some() || source.is_some() { return Err("usage: camel library verify-lock [path]".into()); }
            let path = argument.unwrap_or("camel.lock");
            let lock = libraries::read_lock(path)?;
            println!("lock is valid: {} package(s)", lock.libraries.len());
        }
        "install-locked" => {
            if output.is_some() { return Err("--output is only valid with `lock`".into()); }
            let path = argument.unwrap_or("camel.lock");
            let lock = libraries::read_lock(path)?;
            for installed in libraries::install_locked(&lock)? {
                println!("installed {}", installed.display());
            }
        }
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
            if output.is_some() { return Err("--output is only valid with `lock`".into()); }
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
        approvals: BTreeSet::new(),
        checkpoint: None,
        resume: None,
        record_cassette: None,
        replay_cassette: None,
        evaluation_report: None,
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
            "--workflow"
            | "--steps"
            | "--tool-calls"
            | "--tokens"
            | "--timeout-seconds"
            | "--seconds"
            | "--save-recording"
            | "--checkpoint"
            | "--resume"
            | "--record-cassette"
            | "--replay-cassette"
            | "--evaluation-report" => {
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
                    "--checkpoint" => o.checkpoint = Some(PathBuf::from(value)),
                    "--resume" => o.resume = Some(PathBuf::from(value)),
                    "--record-cassette" => o.record_cassette = Some(PathBuf::from(value)),
                    "--replay-cassette" => o.replay_cassette = Some(PathBuf::from(value)),
                    "--evaluation-report" => o.evaluation_report = Some(PathBuf::from(value)),
                    _ => unreachable!(),
                }
            }
            "--approve" => {
                i += 1;
                let value = args.get(i).ok_or("--approve requires a capability")?;
                let capability = Capability::parse(value)
                    .ok_or_else(|| format!("unknown capability '{value}' for --approve"))?;
                o.approvals.insert(capability);
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
        r#"Camel v.0.3.0 — source-backed agent programming language

  camel check <file.camel>
  camel [run] <file.camel> [--workflow Main] [--json] [--trace] [--steps N] [--tool-calls N] [--tokens N] [--timeout-seconds N] [--approve capability]... -- [arguments]
  camel record <file.camel> [--seconds N|--until-enter] [--save-recording path] [--approve microphone] -- [arguments]
  camel --gui [file.camel]
  camel --update [extracted-distribution-folder]
  camel library [list|search <query>|install <name>|update <name>|remove <name>] [--catalog <path-or-url>]

record is a compatibility command that overrides record(seconds) host input.
Without it, the program controls when and how long to record. Nothing records during check.
--json emits one object containing the workflow value and captured logs.
Model calls use CAMEL_LM_ENDPOINT (default http://localhost:1234), CAMEL_LM_TOKEN,
and CAMEL_LM_TIMEOUT_SECONDS (default 300). Models come from agent declarations.
Effectful CLI runs require explicit per-run approvals: model, microphone, file-read,
file-write, network, and shell. Repeat --approve for each capability the run may use.
Use --checkpoint <path> with checkpoint("label"); statements to save workflow state;
--resume <path> continues after the saved boundary and rejects changed source.
--record-cassette <path> records model, tool, environment and audio boundaries;
--replay-cassette <path> replays without repeating those effects. --evaluation-report
<path> writes a deterministic report and requires replay mode. With --resume, recording
continues from the cassette position saved in the checkpoint.
--gui launches Camel Studio; set CAMEL_STUDIO_PATH to select its executable.
--update opens setup from a local extracted distribution (defaults to the current folder); it does not use the internet.
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

    #[test]
    fn per_run_approvals_are_explicit_and_repeatable() {
        let parsed = options(vec![
            "a.camel".into(),
            "--approve".into(),
            "model".into(),
            "--approve".into(),
            "network".into(),
        ])
        .unwrap();
        assert!(parsed.approvals.contains(&Capability::Model));
        assert!(parsed.approvals.contains(&Capability::Network));
        assert!(options(vec![
            "a.camel".into(),
            "--approve".into(),
            "everything".into(),
        ])
        .is_err());
    }
}
