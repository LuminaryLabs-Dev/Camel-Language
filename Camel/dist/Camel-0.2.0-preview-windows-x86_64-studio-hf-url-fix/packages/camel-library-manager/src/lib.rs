//! Shared catalog, installation, and installed-source lookup for Camel clients.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub const CATALOG_URL: &str =
    "https://raw.githubusercontent.com/LuminaryLabs-Dev/Camel-Language/main/libraries/index.json";
pub const CATALOG_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../libraries/index.json");
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub schema_version: u32,
    pub libraries: Vec<Library>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Library {
    pub name: String,
    pub description: String,
    pub version: String,
    pub revision: String,
    pub repository: String,
    pub path: String,
    pub files: Vec<String>,
}

pub fn installed_root() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os("CAMEL_HOME") {
        return Some(PathBuf::from(root).join("libraries"));
    }
    #[cfg(windows)]
    let base = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Camel");
    #[cfg(not(windows))]
    let base = PathBuf::from(std::env::var_os("HOME")?).join(".camel");
    Some(base.join("libraries"))
}

pub fn installed_library(name: &str) -> Option<PathBuf> {
    if !valid_name(name) {
        return None;
    }
    let path = installed_root()?.join(name);
    path.is_dir().then_some(path)
}

/// Resolve `@camel/<package>/<file>.camel` without downloading or running code.
pub fn resolve_import(import: &str) -> Result<PathBuf> {
    let (name, relative) = import
        .strip_prefix("@camel/")
        .and_then(|s| s.split_once('/'))
        .ok_or("library import must be @camel/<name>/<file>.camel")?;
    if !valid_name(name) || !valid_relative(relative) || !relative.ends_with(".camel") {
        return Err("library import requires a safe package name and relative .camel file".into());
    }
    let root = installed_library(name)
        .ok_or_else(|| {
            format!("Camel library '{name}' is not installed; run `camel library install {name}`")
        })?
        .canonicalize()?;
    let target = root.join(relative).canonicalize()?;
    if !target.starts_with(&root) {
        return Err("library import escapes its installed package".into());
    }
    Ok(target)
}

/// Read a JSON file or URL, preferring this repository's catalog when omitted.
pub fn catalog(source: Option<&str>) -> Result<Catalog> {
    let installed_catalog = std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.parent()
                .and_then(Path::parent)
                .map(|root| root.join("libraries/index.json"))
        })
        .filter(|path| path.is_file());
    let text = match source {
        Some(path) if !path.starts_with("http://") && !path.starts_with("https://") => {
            fs::read_to_string(path)?
        }
        None if installed_catalog.is_some() => fs::read_to_string(installed_catalog.unwrap())?,
        None if Path::new(CATALOG_PATH).is_file() => fs::read_to_string(CATALOG_PATH)?,
        _ => client()?
            .get(source.unwrap_or(CATALOG_URL))
            .send()?
            .error_for_status()?
            .text()?,
    };
    let catalog: Catalog = serde_json::from_str(&text)?;
    if catalog.schema_version != 2 {
        return Err("unsupported Camel catalog schemaVersion; expected 2".into());
    }
    let mut names = BTreeSet::new();
    for library in &catalog.libraries {
        validate(library)?;
        if !names.insert(&library.name) {
            return Err(format!("duplicate library '{}'", library.name).into());
        }
    }
    Ok(catalog)
}

/// Stage a full source snapshot before replacing an installation.
/// Serial operations only: callers must serialize writes to the same package.
pub fn install(library: &Library) -> Result<PathBuf> {
    validate(library)?;
    let root =
        installed_root().ok_or("set CAMEL_HOME to choose a library installation directory")?;
    fs::create_dir_all(&root)?;
    let staging = tempfile::Builder::new()
        .prefix(".camel-library-")
        .tempdir_in(&root)?;
    let client = client()?;
    for relative in &library.files {
        let target = staging.path().join(relative);
        let url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            library.repository.trim_start_matches("https://github.com/"),
            library.revision,
            library.path,
            relative
        );
        let bytes = client.get(url).send()?.error_for_status()?.bytes()?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, bytes)?;
    }
    fs::write(
        staging.path().join(".camel-library.json"),
        serde_json::to_vec_pretty(library)?,
    )?;
    let destination = root.join(&library.name);
    let backup = root.join(format!(".{}.previous", library.name));
    // Recover a previous interrupted replacement before starting another one.
    if backup.exists() && !destination.exists() {
        fs::rename(&backup, &destination)?;
    }
    if backup.exists() {
        fs::remove_dir_all(&backup)?;
    }
    if destination.exists() {
        fs::rename(&destination, &backup)?;
    }
    if let Err(error) = fs::rename(staging.path(), &destination) {
        if backup.exists() {
            fs::rename(&backup, &destination).map_err(|rollback| {
                format!(
                    "install failed: {error}; restore {} manually: {rollback}",
                    backup.display()
                )
            })?;
        }
        return Err(error.into());
    }
    if backup.exists() {
        fs::remove_dir_all(backup)?;
    }
    Ok(destination)
}

/// Remove an installation; returns false when it is already absent.
pub fn remove(name: &str) -> Result<bool> {
    if !valid_name(name) {
        return Err("invalid library name".into());
    }
    let root =
        installed_root().ok_or("set CAMEL_HOME to choose a library installation directory")?;
    let path = root.join(name);
    if !path.exists() {
        return Ok(false);
    }
    fs::remove_dir_all(path)?;
    let backup = root.join(format!(".{name}.previous"));
    if backup.exists() {
        fs::remove_dir_all(backup)?;
    }
    Ok(true)
}

pub fn validate(library: &Library) -> Result<()> {
    let repo = library
        .repository
        .strip_prefix("https://github.com/")
        .unwrap_or("");
    let segments: Vec<_> = repo.split('/').collect();
    let valid_repository = segments.len() == 2
        && segments.iter().all(|s| {
            !s.is_empty()
                && *s != "."
                && *s != ".."
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        });
    let mut files = BTreeSet::new();
    if !valid_name(&library.name)
        || !valid_repository
        || !valid_relative(&library.path)
        || !valid_version(&library.version)
        || !valid_revision(&library.revision)
        || library.files.is_empty()
        || library.files.iter().any(|file| {
            !valid_relative(file)
                || file == ".camel-library.json"
                || !files.insert(file.to_ascii_lowercase())
        })
    {
        return Err(format!("invalid library '{}': expected a safe name, semantic version, Git ref, GitHub HTTPS repository, relative directory, and unique file allowlist", library.name).into());
    }
    Ok(())
}

fn client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("camel-library-manager/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn valid_relative(value: &str) -> bool {
    !value.is_empty()
        && value.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        })
}

fn valid_version(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && (part == &"0" || !part.starts_with('0'))
                && part.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn valid_revision(value: &str) -> bool {
    !value.is_empty()
        && value.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> Library {
        Library {
            name: "standard".into(),
            description: "Helpers".into(),
            version: "1.0.0".into(),
            revision: "main".into(),
            repository: "https://github.com/LuminaryLabs-Dev/Camel-Language".into(),
            path: "libraries/standard".into(),
            files: vec!["core.camel".into()],
        }
    }

    #[test]
    fn package_manifest_rejects_cross_platform_path_escapes() {
        for path in [
            "../outside",
            "C:/outside",
            "..\\outside",
            "/outside",
            "nested/../outside",
        ] {
            let mut item = library();
            item.files = vec![path.into()];
            assert!(validate(&item).is_err(), "{path}");
        }
        assert!(validate(&library()).is_ok());
    }

    #[test]
    fn duplicate_destinations_and_installer_metadata_are_rejected() {
        let mut item = library();
        item.files.push("CORE.camel".into());
        assert!(validate(&item).is_err());
        item.files = vec![".camel-library.json".into()];
        assert!(validate(&item).is_err());
    }

    #[test]
    fn library_versions_and_revisions_are_checked_before_download() {
        let mut item = library();
        item.version = "latest".into();
        assert!(validate(&item).is_err());
        item.version = "01.2.3".into();
        assert!(validate(&item).is_err());
        item.version = "1.2.3".into();
        item.revision = "../../main".into();
        assert!(validate(&item).is_err());
        item.revision = "release/audio-v1".into();
        assert!(validate(&item).is_ok());
    }

    #[test]
    fn remove_rejects_root_and_parent_without_touching_disk() {
        for name in ["", ".", "..", "../standard", "C:\\"] {
            assert!(remove(name).is_err());
            assert!(installed_library(name).is_none());
        }
    }
}
