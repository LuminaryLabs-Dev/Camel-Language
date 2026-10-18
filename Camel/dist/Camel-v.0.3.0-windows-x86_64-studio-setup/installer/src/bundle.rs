use crate::Result;
use ring::digest::{Context, SHA256};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format_version: u32,
    pub version: String,
    pub os: String,
    pub arch: String,
    pub files: Vec<File>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct File {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug)]
pub struct Bundle {
    pub root: PathBuf,
    pub manifest: Manifest,
}

impl Bundle {
    pub fn load(root: &Path) -> Result<Self> {
        let root = crate::canonicalize(root)?;
        let manifest: Manifest =
            serde_json::from_slice(&fs::read(root.join("INSTALLER_BUNDLE.json"))?)?;
        if manifest.format_version != 1 || manifest.version.is_empty() || manifest.files.is_empty()
        {
            return Err("This distribution has an unsupported or empty installer manifest.".into());
        }
        if manifest.os != std::env::consts::OS || manifest.arch != std::env::consts::ARCH {
            return Err(format!(
                "This bundle is for {} {}. Download the {} {} distribution.",
                manifest.os,
                manifest.arch,
                std::env::consts::OS,
                std::env::consts::ARCH
            )
            .into());
        }
        let mut names = BTreeSet::new();
        for entry in &manifest.files {
            if !safe_relative(&entry.path)
                || !names.insert(entry.path.to_lowercase())
                || entry.sha256.len() != 64
                || !entry.sha256.bytes().all(|c| c.is_ascii_hexdigit())
            {
                return Err(format!("Invalid installer entry: {}", entry.path).into());
            }
        }
        if !manifest
            .files
            .iter()
            .any(|f| f.path == format!("bin/camel{}", std::env::consts::EXE_SUFFIX))
        {
            return Err("The bundle does not contain the Camel executable.".into());
        }
        Ok(Self { root, manifest })
    }

    pub fn verify(&self) -> Result<()> {
        for entry in &self.manifest.files {
            let path = crate::canonicalize(&self.root.join(&entry.path))?;
            if !path.starts_with(&self.root) || !path.is_file() {
                return Err(
                    format!("Bundle file is outside the distribution: {}", entry.path).into(),
                );
            }
            if hash(&path)? != entry.sha256.to_lowercase() {
                return Err(format!(
                    "Checksum failed for {}. Extract a fresh distribution.",
                    entry.path
                )
                .into());
            }
        }
        Ok(())
    }

    pub fn includes_studio(&self) -> bool {
        self.manifest
            .files
            .iter()
            .any(|f| f.path == format!("bin/camel-studio{}", std::env::consts::EXE_SUFFIX))
    }
}

pub fn safe_relative(value: &str) -> bool {
    !value.is_empty()
        && !value.contains(['\\', ':'])
        && !Path::new(value).is_absolute()
        && Path::new(value)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

pub fn hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Context::new(&SHA256);
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher
        .finish()
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_paths_that_escape_on_either_platform() {
        for value in [
            "../camel",
            "bin/../camel",
            "/bin/camel",
            "C:/camel.exe",
            "bin\\camel.exe",
            "bin//camel",
        ] {
            assert!(!safe_relative(value), "{value}");
        }
        assert!(safe_relative("bin/camel.exe"));
    }

    #[test]
    fn modified_payload_is_rejected_before_installation() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("camel");
        fs::write(&path, b"original").unwrap();
        let bundle = Bundle {
            root: crate::canonicalize(temp.path()).unwrap(),
            manifest: Manifest {
                format_version: 1,
                version: "test".into(),
                os: std::env::consts::OS.into(),
                arch: std::env::consts::ARCH.into(),
                files: vec![File {
                    path: "camel".into(),
                    sha256: hash(&path).unwrap(),
                }],
            },
        };
        bundle.verify().unwrap();
        fs::write(path, b"changed").unwrap();
        assert!(bundle.verify().is_err());
    }
}
