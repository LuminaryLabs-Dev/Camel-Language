use crate::Result;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(any(unix, test))]
const START: &str = "# >>> Camel PATH >>>";
#[cfg(any(unix, test))]
const END: &str = "# <<< Camel PATH <<<";

#[derive(Clone)]
struct Edit {
    path: PathBuf,
    before: Option<Vec<u8>>,
    after: Vec<u8>,
}

pub struct Plan {
    files: Vec<Edit>,
    #[cfg(windows)]
    registry: Option<(Option<String>, String)>,
}

impl Plan {
    pub fn prepare(bin: &Path, enabled: bool) -> Result<Self> {
        if !enabled {
            return Ok(Self {
                files: Vec::new(),
                #[cfg(windows)]
                registry: None,
            });
        }
        #[cfg(windows)]
        {
            let before = windows_path(None)?;
            let after = add_windows_entry(before.as_deref().unwrap_or(""), &bin.to_string_lossy());
            Ok(Self {
                files: Vec::new(),
                registry: Some((before, after)),
            })
        }
        #[cfg(unix)]
        {
            let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set.")?);
            let zdir = std::env::var_os("ZDOTDIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.clone());
            let config = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"));
            unix_plan(&home, &zdir, &config, bin)
        }
    }

    pub fn apply(&self) -> Result<()> {
        let mut changed = Vec::new();
        for edit in &self.files {
            if let Err(error) = write_atomic(&edit.path, &edit.after) {
                for previous in changed.into_iter().rev() {
                    restore(previous)?;
                }
                return Err(error);
            }
            changed.push(edit);
        }
        #[cfg(windows)]
        if let Some((_, after)) = &self.registry {
            if let Err(error) = windows_path(Some(Some(after))) {
                self.rollback()?;
                return Err(error);
            }
        }
        Ok(())
    }

    #[cfg(any(windows, test))]
    pub fn rollback(&self) -> Result<()> {
        for edit in self.files.iter().rev() {
            restore(edit)?;
        }
        #[cfg(windows)]
        if let Some((before, _)) = &self.registry {
            windows_path(Some(before.as_deref()))?;
        }
        Ok(())
    }

    pub fn locations(&self) -> Vec<String> {
        #[cfg(windows)]
        if self.registry.is_some() {
            return vec!["Windows user PATH".into()];
        }
        self.files
            .iter()
            .map(|e| e.path.display().to_string())
            .collect()
    }
}

#[cfg(any(unix, test))]
fn unix_plan(home: &Path, zdir: &Path, config: &Path, bin: &Path) -> Result<Plan> {
    let quoted = shell_quote(&bin.to_string_lossy());
    let block = format!("{START}\ncase \":${{PATH:-}}:\" in\n    *:{quoted}:*) ;;\n    *) export PATH={quoted}${{PATH:+:\"$PATH\"}} ;;\nesac\n{END}");
    let mut paths = vec![
        home.join(".profile"),
        home.join(".bashrc"),
        zdir.join(".zprofile"),
        zdir.join(".zshrc"),
    ];
    for name in [".bash_profile", ".bash_login"] {
        let path = home.join(name);
        if path.exists() {
            paths.push(path);
        }
    }
    let fish_path = config.join("fish/conf.d/camel.fish");
    let fish = format!(
        "{START}\nfish_add_path --global {}\n{END}",
        fish_quote(&bin.to_string_lossy())
    );
    paths.push(fish_path.clone());
    let mut files = Vec::new();
    for path in paths {
        let is_fish = path == fish_path;
        // Preserve dotfile symlinks by writing their existing target.
        let path = if path.exists() {
            path.canonicalize()?
        } else {
            path
        };
        if files.iter().any(|edit: &Edit| edit.path == path) {
            continue;
        }
        let before = if path.exists() {
            Some(fs::read(&path)?)
        } else {
            None
        };
        let source = std::str::from_utf8(before.as_deref().unwrap_or_default())?;
        let after = replace_block(source, if is_fish { &fish } else { &block })?.into_bytes();
        files.push(Edit {
            path,
            before,
            after,
        });
    }
    Ok(Plan {
        files,
        #[cfg(windows)]
        registry: None,
    })
}

#[cfg(any(unix, test))]
fn replace_block(source: &str, replacement: &str) -> Result<String> {
    let mut result = source.to_string();
    if let Some(start) = result.find(START) {
        let end = result[start..]
            .find(END)
            .ok_or("An existing Camel PATH block is incomplete. Repair it before installing.")?
            + start
            + END.len();
        if result[..start].contains(END)
            || result[end..].contains(START)
            || result[end..].contains(END)
        {
            return Err(
                "Multiple or incomplete Camel PATH blocks exist. Repair them before installing."
                    .into(),
            );
        }
        result.replace_range(start..end, replacement);
        return Ok(result);
    }
    if result.contains(END) {
        return Err("An existing Camel PATH block is incomplete.".into());
    }
    if !result.is_empty() && !result.ends_with('\n') {
        result.push('\n');
    }
    result.push_str(replacement);
    result.push('\n');
    Ok(result)
}

#[cfg(any(unix, test))]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
#[cfg(any(unix, test))]
fn fish_quote(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

#[cfg(any(windows, test))]
fn add_windows_entry(current: &str, bin: &str) -> String {
    let normalize = |s: &str| {
        s.trim()
            .trim_matches('"')
            .trim_end_matches(['\\', '/'])
            .to_lowercase()
    };
    if current
        .split(';')
        .any(|entry| normalize(entry) == normalize(bin))
    {
        return current.into();
    }
    if current.is_empty() {
        bin.into()
    } else {
        format!("{};{bin}", current.trim_end_matches(';'))
    }
}

#[cfg(windows)]
fn windows_path(value: Option<Option<&str>>) -> Result<Option<String>> {
    use std::{os::windows::process::CommandExt, process::Command};
    // Values travel through environment variables, never through script interpolation.
    let script = if value.is_some() {
        "$ErrorActionPreference='Stop'; $value = if ($env:CAMEL_SETUP_CLEAR_PATH -eq '1') { $null } else { $env:CAMEL_SETUP_USER_PATH }; [Environment]::SetEnvironmentVariable('Path', $value, 'User'); ConvertTo-Json -InputObject ([Environment]::GetEnvironmentVariable('Path','User')) -Compress"
    } else {
        "$ErrorActionPreference='Stop'; ConvertTo-Json -InputObject ([Environment]::GetEnvironmentVariable('Path','User')) -Compress"
    };
    let mut command = Command::new("powershell.exe");
    let script = format!("[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); {script}");
    command.creation_flags(0x08000000).args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &script,
    ]);
    if let Some(value) = value {
        command.env("CAMEL_SETUP_USER_PATH", value.unwrap_or(""));
        command.env(
            "CAMEL_SETUP_CLEAR_PATH",
            if value.is_none() { "1" } else { "0" },
        );
    }
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "Windows PATH update failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn restore(edit: &Edit) -> Result<()> {
    match &edit.before {
        Some(bytes) => write_atomic(&edit.path, bytes),
        None => {
            if edit.path.exists() {
                fs::remove_file(&edit.path)?;
            }
            Ok(())
        }
    }
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path.parent().ok_or("Missing file parent.")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    if path.exists() {
        file.as_file()
            .set_permissions(fs::metadata(path)?.permissions())?;
    }
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_path_preserves_existing_entries_and_does_not_duplicate() {
        let existing = "C:\\Tools;%USERPROFILE%\\bin;C:\\Camel\\bin\\";
        assert_eq!(add_windows_entry(existing, "c:\\camel\\bin"), existing);
        assert_eq!(
            add_windows_entry("C:\\Tools", "C:\\Camel\\bin"),
            "C:\\Tools;C:\\Camel\\bin"
        );
    }

    #[test]
    fn unix_updates_are_repeatable_and_rollback_preserves_dotfiles() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let profile = home.join(".profile");
        let original = b"# my configuration\nexport EDITOR=vi\n";
        fs::write(&profile, original).unwrap();
        let bin = home.join("Camel's install/bin");
        let first = unix_plan(home, home, &home.join(".config"), &bin).unwrap();
        first.apply().unwrap();
        let first_contents = fs::read_to_string(&profile).unwrap();
        let second = unix_plan(home, home, &home.join(".config"), &bin).unwrap();
        second.apply().unwrap();
        let updated = fs::read_to_string(&profile).unwrap();
        assert_eq!(updated, first_contents);
        assert_eq!(updated.matches(START).count(), 1);
        assert!(updated.contains("export EDITOR=vi"));
        first.rollback().unwrap();
        assert_eq!(fs::read(&profile).unwrap(), original);
        assert!(!home.join(".bashrc").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_real_shell_loads_a_quoted_path_once() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let bin = home.join("Camel's folder/bin");
        let plan = unix_plan(home, home, &home.join(".config"), &bin).unwrap();
        plan.apply().unwrap();
        let output = std::process::Command::new("/bin/sh")
            .args([
                "-c",
                ". \"$1\"; . \"$1\"; printf '%s' \"$PATH\"",
                "camel-path-test",
            ])
            .arg(home.join(".profile"))
            .env("PATH", "/usr/bin:/bin")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("{}:/usr/bin:/bin", bin.display())
        );
    }
}
