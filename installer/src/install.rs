use crate::{bundle::Bundle, path::Plan, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone)]
pub struct Request {
    pub destination: PathBuf,
    pub add_to_path: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub product: String,
    pub version: String,
    pub destination: PathBuf,
    pub path_locations: Vec<String>,
}

pub fn default_destination() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        Ok(
            PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set.")?)
                .join("Programs/Camel"),
        )
    }
    #[cfg(target_os = "macos")]
    {
        Ok(
            PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set.")?)
                .join("Library/Application Support/Camel"),
        )
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or(
                PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set.")?)
                    .join(".local/share"),
            );
        Ok(base.join("camel"))
    }
}

pub fn install(bundle: &Bundle, request: &Request, progress: impl Fn(&str)) -> Result<Receipt> {
    progress("Checking distribution files…");
    bundle.verify()?;
    let destination = destination(bundle, &request.destination)?;
    let bin = destination.join("bin");
    let plan = Plan::prepare(&bin, request.add_to_path)?;
    let parent = destination
        .parent()
        .ok_or("Choose an installation folder with a parent.")?;
    fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".camel-setup-")
        .tempdir_in(parent)?;
    progress("Preparing Camel in a temporary folder…");
    for file in &bundle.manifest.files {
        let target = staging.path().join(&file.path);
        fs::create_dir_all(target.parent().ok_or("Missing payload parent.")?)?;
        fs::copy(bundle.root.join(&file.path), &target)?;
        if crate::bundle::hash(&target)? != file.sha256.to_lowercase() {
            return Err(format!("Bundle file changed while copying: {}", file.path).into());
        }
        if file.path.starts_with("bin/") {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&target, fs::Permissions::from_mode(0o755))?;
            }
        }
    }
    progress("Checking the installed Camel executable…");
    let version = version(
        &staging
            .path()
            .join(format!("bin/camel{}", std::env::consts::EXE_SUFFIX)),
    )?;
    if !version.starts_with(&format!("Camel {}", bundle.manifest.version)) {
        return Err(format!(
            "The executable reports '{version}', but the bundle version is {}.",
            bundle.manifest.version
        )
        .into());
    }
    let receipt = Receipt {
        product: "Camel".into(),
        version: bundle.manifest.version.clone(),
        destination: destination.clone(),
        path_locations: plan.locations(),
    };
    fs::write(
        staging.path().join("INSTALLATION.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    progress("Installing Camel and updating PATH…");
    commit(staging.path(), &destination, &plan)?;
    progress("Installation complete.");
    Ok(receipt)
}

pub fn version(executable: &Path) -> Result<String> {
    let mut command = Command::new(executable);
    command.arg("--version");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "Camel could not start: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().into())
}

fn destination(bundle: &Bundle, requested: &Path) -> Result<PathBuf> {
    let text = requested.to_string_lossy();
    if !requested.is_absolute()
        || requested.file_name().is_none()
        || text.contains(['\n', '\r'])
        || (cfg!(windows) && text.contains(';'))
        || (cfg!(unix) && text.contains(':'))
        || requested
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(
            "Choose an absolute installation folder without parent traversal or PATH separators."
                .into(),
        );
    }
    let mut ancestor = requested;
    let mut tail = Vec::new();
    while !ancestor.exists() {
        tail.push(ancestor.file_name().ok_or("Invalid installation folder.")?);
        ancestor = ancestor.parent().ok_or("Missing installation parent.")?;
    }
    let mut resolved = crate::canonicalize(ancestor)?;
    for part in tail.into_iter().rev() {
        resolved.push(part);
    }
    if resolved.starts_with(&bundle.root) || bundle.root.starts_with(&resolved) {
        return Err(
            "Choose a separate installation folder outside the extracted distribution.".into(),
        );
    }
    if resolved.exists() {
        if !resolved.is_dir() {
            return Err("The installation location is a file.".into());
        }
        if fs::read_dir(&resolved)?.next().is_some() {
            let marker = resolved.join("INSTALLATION.json");
            let receipt: Receipt = serde_json::from_slice(&fs::read(marker).map_err(|_| "Choose an empty folder or an existing Camel installation; other files will not be replaced.")?)?;
            if receipt.product != "Camel" || receipt.destination != resolved {
                return Err("This folder is not a matching Camel installation.".into());
            }
        }
    }
    Ok(resolved)
}

fn commit(staging: &Path, destination: &Path, plan: &Plan) -> Result<()> {
    let parent = destination.parent().ok_or("Missing installation parent.")?;
    let holder = tempfile::Builder::new()
        .prefix(".camel-backup-")
        .tempdir_in(parent)?;
    let backup = holder.path().join("previous");
    let had_previous = destination.exists();
    if had_previous {
        if let Err(error) = fs::rename(destination, &backup) {
            #[cfg(windows)]
            {
                return commit_in_place(staging, destination, plan, holder.path()).map_err(
                    |fallback| {
                        format!(
                            "Could not replace the existing Camel folder {} ({error}); file-by-file update also failed: {fallback}",
                            destination.display()
                        )
                        .into()
                    },
                );
            }
            #[cfg(not(windows))]
            return Err(format!(
                "Could not move the existing Camel folder {} aside for replacement: {error}",
                destination.display()
            )
            .into());
        }
    }
    if let Err(error) = fs::rename(staging, destination) {
        if had_previous {
            if let Err(restore) = fs::rename(&backup, destination) {
                let kept = holder.keep();
                return Err(format!(
                    "Install failed: {error}. Previous installation is retained in {}: {restore}",
                    kept.display()
                )
                .into());
            }
        }
        return Err(error.into());
    }
    if let Err(error) = plan.apply() {
        if let Err(remove) = fs::remove_dir_all(destination) {
            let kept = holder.keep();
            return Err(format!("PATH setup failed: {error}. Cleanup failed: {remove}. Previous installation is retained in {}.", kept.display()).into());
        }
        if had_previous {
            if let Err(restore) = fs::rename(&backup, destination) {
                let kept = holder.keep();
                return Err(format!("PATH setup failed: {error}. Restore the previous installation from {}: {restore}", kept.display()).into());
            }
        }
        return Err(error);
    }
    // Backup cleanup is managed by the temporary directory after a successful commit.
    Ok(())
}

#[cfg(windows)]
fn commit_in_place(
    staging: &Path,
    destination: &Path,
    plan: &Plan,
    backup_root: &Path,
) -> Result<()> {
    use tempfile::NamedTempFile;

    #[derive(Debug)]
    struct ChangedFile {
        destination: PathBuf,
        backup: Option<PathBuf>,
    }

    fn replace(source: &Path, destination: &Path) -> Result<()> {
        let parent = destination.parent().ok_or("Missing destination parent.")?;
        fs::create_dir_all(parent)?;
        let temporary = NamedTempFile::new_in(parent)?;
        fs::copy(source, temporary.path())?;
        temporary
            .persist(destination)
            .map_err(|error| error.error)?;
        Ok(())
    }

    fn rollback(changed: &[ChangedFile]) -> Result<()> {
        for file in changed.iter().rev() {
            if let Some(backup) = &file.backup {
                replace(backup, &file.destination).map_err(|error| {
                    format!("Could not restore {}: {error}", file.destination.display())
                })?;
            } else if file.destination.exists() {
                fs::remove_file(&file.destination).map_err(|error| {
                    format!("Could not remove {}: {error}", file.destination.display())
                })?;
            }
        }
        Ok(())
    }

    fn files_under(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            let file_type = fs::symlink_metadata(&path)?.file_type();
            if file_type.is_dir() {
                files_under(root, &path, files)?;
            } else if file_type.is_file() {
                files.push(path.strip_prefix(root)?.to_path_buf());
            } else {
                return Err(format!("Unsupported staged file type: {}", path.display()).into());
            }
        }
        Ok(())
    }

    fs::create_dir_all(backup_root)?;
    let mut files = Vec::new();
    files_under(staging, staging, &mut files)?;
    let mut changed = Vec::new();
    let update = (|| -> Result<()> {
        for relative in files {
            let source = staging.join(&relative);
            let target = destination.join(&relative);
            let backup = if target.exists() {
                let path = backup_root.join(&relative);
                fs::create_dir_all(path.parent().ok_or("Missing backup parent.")?)?;
                fs::copy(&target, &path)
                    .map_err(|error| format!("Could not back up {}: {error}", target.display()))?;
                Some(path)
            } else {
                None
            };
            replace(&source, &target)
                .map_err(|error| format!("Could not replace {}: {error}", target.display()))?;
            changed.push(ChangedFile {
                destination: target,
                backup,
            });
        }
        Ok(())
    })();
    if let Err(error) = update {
        rollback(&changed).map_err(|restore| {
            format!("File-by-file update failed: {error}. File rollback failed: {restore}")
        })?;
        return Err(error);
    }

    if let Err(error) = plan.apply() {
        rollback(&changed).map_err(|restore| {
            format!("PATH setup failed: {error}. File rollback failed: {restore}")
        })?;
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::Manifest;

    #[test]
    fn an_existing_unrelated_folder_cannot_be_replaced() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("bundle");
        let target = temp.path().join("personal files");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("important.txt"), b"keep me").unwrap();
        let bundle = Bundle {
            root: crate::canonicalize(&root).unwrap(),
            manifest: Manifest {
                format_version: 1,
                version: "test".into(),
                os: std::env::consts::OS.into(),
                arch: std::env::consts::ARCH.into(),
                files: Vec::new(),
            },
        };
        assert!(destination(&bundle, &target).is_err());
        assert_eq!(fs::read(target.join("important.txt")).unwrap(), b"keep me");
        assert!(destination(&bundle, &root).is_err());
    }

    #[test]
    fn failed_final_rename_restores_the_previous_installation() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("Camel");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("old"), b"previous version").unwrap();
        let plan = Plan::prepare(&target.join("bin"), false).unwrap();
        assert!(commit(&temp.path().join("missing staging"), &target, &plan).is_err());
        assert_eq!(fs::read(target.join("old")).unwrap(), b"previous version");
    }
}
