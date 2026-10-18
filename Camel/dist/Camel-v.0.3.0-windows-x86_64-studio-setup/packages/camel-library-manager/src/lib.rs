//! Shared catalog, installation, and installed-source lookup for Camel clients.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
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
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub name: String,
    pub description: String,
    pub version: String,
    pub revision: String,
    pub repository: String,
    pub path: String,
    pub files: Vec<String>,
    #[serde(default)]
    pub file_checksums: BTreeMap<String, String>,
    #[serde(default)]
    pub dependencies: Vec<LibraryDependency>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct LibraryDependency {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageLock {
    pub lock_version: u32,
    pub roots: Vec<LibraryDependency>,
    /// Dependencies precede the packages that require them.
    pub libraries: Vec<Library>,
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
    if catalog.schema_version != 3 {
        return Err("unsupported Camel catalog schemaVersion; expected 3".into());
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
        verify_file_checksum(library, relative, &bytes)?;
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

/// Resolve exact direct and transitive versions into a deterministic lock.
pub fn create_lock(catalog: &Catalog, roots: &[String]) -> Result<PackageLock> {
    if catalog.schema_version != 3 {
        return Err("unsupported Camel catalog schemaVersion; expected 3".into());
    }
    let mut root_names = roots.to_vec();
    root_names.sort();
    root_names.dedup();
    if root_names.is_empty() {
        return Err("a lock needs at least one root library".into());
    }
    let by_name = catalog
        .libraries
        .iter()
        .map(|library| (library.name.as_str(), library))
        .collect::<BTreeMap<_, _>>();
    if by_name.len() != catalog.libraries.len() {
        return Err("catalog contains duplicate library names".into());
    }
    let mut lock_roots = Vec::new();
    let mut ordered = Vec::new();
    let mut complete = BTreeSet::new();
    let mut active = BTreeSet::new();
    for name in root_names {
        let library = by_name
            .get(name.as_str())
            .ok_or_else(|| format!("library '{name}' is not in the catalog"))?;
        lock_roots.push(LibraryDependency {
            name: name.clone(),
            version: library.version.clone(),
        });
        add_dependencies(library, &by_name, &mut active, &mut complete, &mut ordered)?;
    }
    Ok(PackageLock {
        lock_version: 1,
        roots: lock_roots,
        libraries: ordered,
    })
}

fn add_dependencies(
    library: &Library,
    by_name: &BTreeMap<&str, &Library>,
    active: &mut BTreeSet<String>,
    complete: &mut BTreeSet<String>,
    ordered: &mut Vec<Library>,
) -> Result<()> {
    validate(library)?;
    if complete.contains(&library.name) {
        return Ok(());
    }
    if !active.insert(library.name.clone()) {
        return Err(format!("library dependency cycle includes '{}'", library.name).into());
    }
    let mut dependencies = library.dependencies.clone();
    dependencies.sort_by(|left, right| left.name.cmp(&right.name));
    for dependency in dependencies {
        let required = by_name.get(dependency.name.as_str()).ok_or_else(|| {
            format!(
                "library '{}' requires missing '{}'",
                library.name, dependency.name
            )
        })?;
        if required.version != dependency.version {
            return Err(format!(
                "library '{}' requires {} {}, catalog provides {}",
                library.name, dependency.name, dependency.version, required.version
            )
            .into());
        }
        add_dependencies(required, by_name, active, complete, ordered)?;
    }
    active.remove(&library.name);
    complete.insert(library.name.clone());
    let mut canonical = library.clone();
    canonical
        .dependencies
        .sort_by(|left, right| left.name.cmp(&right.name));
    ordered.push(canonical);
    Ok(())
}

/// Validate lock pins, exact dependency versions, content checksums, and acyclic order.
pub fn verify_lock(lock: &PackageLock) -> Result<()> {
    if lock.lock_version != 1 || lock.roots.is_empty() {
        return Err("unsupported or empty Camel lockfile".into());
    }
    if lock
        .roots
        .windows(2)
        .any(|pair| pair[0].name >= pair[1].name)
    {
        return Err("lock roots must be unique and sorted by package name".into());
    }
    let mut names = BTreeSet::new();
    let by_name = lock
        .libraries
        .iter()
        .map(|library| (library.name.as_str(), library))
        .collect::<BTreeMap<_, _>>();
    if by_name.len() != lock.libraries.len() {
        return Err("lockfile contains duplicate library names".into());
    }
    for library in &lock.libraries {
        validate(library)?;
        if library
            .dependencies
            .windows(2)
            .any(|pair| pair[0].name >= pair[1].name)
        {
            return Err(format!(
                "dependencies for '{}' must be unique and sorted",
                library.name
            )
            .into());
        }
        names.insert(library.name.as_str());
    }
    for root in &lock.roots {
        let library = by_name
            .get(root.name.as_str())
            .ok_or_else(|| format!("lock root '{}' is missing", root.name))?;
        if library.version != root.version {
            return Err(format!("lock root '{}' has the wrong version", root.name).into());
        }
    }
    let mut ordered = Vec::new();
    let mut active = BTreeSet::new();
    let mut complete = BTreeSet::new();
    for root in &lock.roots {
        let library = by_name[root.name.as_str()];
        add_dependencies(library, &by_name, &mut active, &mut complete, &mut ordered)?;
    }
    if ordered.len() != names.len() {
        return Err("lockfile contains packages not reachable from its roots".into());
    }
    if lock
        .libraries
        .iter()
        .map(|library| library.name.as_str())
        .ne(ordered.iter().map(|library| library.name.as_str()))
    {
        return Err("lockfile packages are not in deterministic dependency order".into());
    }
    Ok(())
}

pub fn read_lock(path: impl AsRef<Path>) -> Result<PackageLock> {
    let lock: PackageLock = serde_json::from_slice(&fs::read(path)?)?;
    verify_lock(&lock)?;
    Ok(lock)
}

pub fn write_lock(path: impl AsRef<Path>, lock: &PackageLock) -> Result<()> {
    verify_lock(lock)?;
    let path = path.as_ref();
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut pending = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut pending, lock)?;
    pending.as_file().sync_all()?;
    pending.persist(path).map_err(|error| error.error)?;
    Ok(())
}

/// Install every package pinned in a verified lock, in dependency order.
pub fn install_locked(lock: &PackageLock) -> Result<Vec<PathBuf>> {
    verify_lock(lock)?;
    lock.libraries.iter().map(install).collect()
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
    let mut dependencies = BTreeSet::new();
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
        || !is_immutable_revision(&library.revision)
        || library.file_checksums.len() != library.files.len()
        || library.files.iter().any(|file| {
            library
                .file_checksums
                .get(file)
                .is_none_or(|checksum| !valid_sha256(checksum))
        })
        || library.dependencies.iter().any(|dependency| {
            !valid_name(&dependency.name)
                || !valid_version(&dependency.version)
                || !dependencies.insert(dependency.name.as_str())
        })
    {
        return Err(format!("invalid library '{}': expected a safe name, semantic version, immutable commit revision, GitHub HTTPS repository, relative directory, unique file allowlist, SHA-256 checksums for every file, and valid dependencies", library.name).into());
    }
    Ok(())
}

fn verify_file_checksum(library: &Library, relative: &str, bytes: &[u8]) -> Result<()> {
    let expected = library.file_checksums.get(relative).ok_or_else(|| {
        format!(
            "library '{}' has no checksum for '{relative}'",
            library.name
        )
    })?;
    let actual = ring::digest::digest(&ring::digest::SHA256, bytes);
    let actual = actual
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(format!(
            "checksum mismatch for library '{}' file '{relative}'",
            library.name
        )
        .into());
    }
    Ok(())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_immutable_revision(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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
            revision: "0123456789abcdef0123456789abcdef01234567".into(),
            repository: "https://github.com/LuminaryLabs-Dev/Camel-Language".into(),
            path: "libraries/standard".into(),
            files: vec!["core.camel".into()],
            file_checksums: BTreeMap::from([("core.camel".into(), "a".repeat(64))]),
            dependencies: Vec::new(),
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
        item.revision = "0123456789abcdef0123456789abcdef01234567".into();
        assert!(validate(&item).is_ok());
    }

    #[test]
    fn lock_resolution_is_deterministic_and_dependency_first() {
        let mut dependency = library();
        dependency.name = "base".into();
        dependency.version = "1.2.0".into();
        dependency.path = "libraries/base".into();
        let mut root = library();
        root.dependencies = vec![LibraryDependency {
            name: "base".into(),
            version: "1.2.0".into(),
        }];
        let catalog = Catalog {
            schema_version: 3,
            libraries: vec![root, dependency],
        };
        let first = create_lock(&catalog, &["standard".into()]).unwrap();
        let second = create_lock(&catalog, &["standard".into()]).unwrap();
        assert_eq!(
            serde_json::to_vec(&first).unwrap(),
            serde_json::to_vec(&second).unwrap()
        );
        assert_eq!(first.libraries[0].name, "base");
        assert_eq!(first.libraries[1].name, "standard");
        verify_lock(&first).unwrap();
    }

    #[test]
    fn dependency_cycles_are_rejected() {
        let mut first = library();
        first.dependencies = vec![LibraryDependency {
            name: "second".into(),
            version: "1.0.0".into(),
        }];
        let mut second = library();
        second.name = "second".into();
        second.dependencies = vec![LibraryDependency {
            name: "standard".into(),
            version: "1.0.0".into(),
        }];
        let catalog = Catalog {
            schema_version: 3,
            libraries: vec![first, second],
        };
        assert!(create_lock(&catalog, &["standard".into()]).is_err());
    }

    #[test]
    fn checked_in_catalog_has_verified_lock_metadata() {
        let catalog = catalog(Some(CATALOG_PATH)).unwrap();
        let lock = create_lock(&catalog, &["standard".into(), "audio".into()]).unwrap();
        verify_lock(&lock).unwrap();
        assert_eq!(lock.libraries.len(), 2);
    }

    #[test]
    fn lock_rejects_dependency_version_mismatch_and_checksum_corruption() {
        let mut root = library();
        root.dependencies = vec![LibraryDependency {
            name: "missing".into(),
            version: "1.0.0".into(),
        }];
        let catalog = Catalog {
            schema_version: 3,
            libraries: vec![root.clone()],
        };
        assert!(create_lock(&catalog, &["standard".into()]).is_err());
        root.dependencies.clear();
        let lock = PackageLock {
            lock_version: 1,
            roots: vec![LibraryDependency {
                name: "standard".into(),
                version: "1.0.0".into(),
            }],
            libraries: vec![root],
        };
        let mut corrupted = lock;
        corrupted.libraries[0]
            .file_checksums
            .insert("core.camel".into(), "b".repeat(64));
        assert!(verify_file_checksum(&corrupted.libraries[0], "core.camel", b"contents").is_err());
        assert!(verify_lock(&corrupted).is_ok());
    }

    #[test]
    fn lock_round_trips_through_atomic_file_write() {
        let lock = create_lock(
            &Catalog {
                schema_version: 3,
                libraries: vec![library()],
            },
            &["standard".into()],
        )
        .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("camel.lock");
        write_lock(&path, &lock).unwrap();
        assert_eq!(read_lock(&path).unwrap().libraries[0].name, "standard");
        write_lock(&path, &lock).unwrap();
        assert_eq!(read_lock(&path).unwrap().roots, lock.roots);
    }

    #[test]
    fn source_install_checksum_matches_actual_content() {
        let mut item = library();
        let digest = ring::digest::digest(&ring::digest::SHA256, b"contents");
        item.file_checksums.insert(
            "core.camel".into(),
            digest
                .as_ref()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        );
        assert!(verify_file_checksum(&item, "core.camel", b"contents").is_ok());
        assert!(verify_file_checksum(&item, "core.camel", b"changed").is_err());
    }

    #[test]
    fn remove_rejects_root_and_parent_without_touching_disk() {
        for name in ["", ".", "..", "../standard", "C:\\"] {
            assert!(remove(name).is_err());
            assert!(installed_library(name).is_none());
        }
    }
}
