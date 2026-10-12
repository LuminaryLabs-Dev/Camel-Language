//! One owner for an editor buffer and its last committed disk snapshot.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub struct Document {
    pub path: Option<PathBuf>,
    pub text: String,
    baseline: Option<Vec<u8>>,
}
impl Document {
    pub fn new(text: String) -> Self {
        Self {
            path: None,
            text,
            baseline: None,
        }
    }
    pub fn open(path: &Path) -> Result<Self, String> {
        let path = path.canonicalize().map_err(|e| e.to_string())?;
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        if bytes.len() > 1_048_576 {
            return Err("Studio supports scripts up to 1 MiB".into());
        }
        let text = String::from_utf8(bytes.clone())
            .map_err(|_| "Camel scripts must be UTF-8".to_owned())?;
        Ok(Self {
            path: Some(path),
            text,
            baseline: Some(bytes),
        })
    }
    pub fn dirty(&self) -> bool {
        self.baseline.as_deref() != Some(self.text.as_bytes())
    }
    pub fn name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled.camel".into())
    }
    pub fn save(&mut self, target: &Path) -> Result<(), String> {
        if self.text.len() > 1_048_576 {
            return Err("Studio supports scripts up to 1 MiB; edits remain in memory.".into());
        }
        let parent = target
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let name = target.file_name().ok_or("Missing filename")?;
        let target = parent.join(name);
        let resolved = if target.exists() {
            target.canonicalize().map_err(|e| e.to_string())?
        } else {
            target.clone()
        };
        if self.path.as_ref() == Some(&resolved) {
            let disk = fs::read(&resolved).map_err(|e| format!("Cannot verify saved file: {e}"))?;
            if self.baseline.as_ref() != Some(&disk) {
                return Err("File changed outside Studio. Close and reopen it, or Save As a new file; your edits are still in memory.".into());
            }
            if disk == self.text.as_bytes() {
                return Ok(());
            }
        } else if resolved.exists() {
            // Never silently replace another script, including a Save As destination.
            return Err(
                "Destination already exists. Choose a new filename or open that file first.".into(),
            );
        }
        let mut pending = tempfile::NamedTempFile::new_in(resolved.parent().unwrap())
            .map_err(|e| e.to_string())?;
        pending
            .write_all(self.text.as_bytes())
            .and_then(|_| pending.flush())
            .and_then(|_| pending.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        if self.path.as_ref() == Some(&resolved) {
            pending.persist(&resolved).map_err(|e| e.to_string())?;
        } else {
            pending
                .persist_noclobber(&resolved)
                .map_err(|e| e.to_string())?;
        }
        // Publish path + snapshot only after the disk commit succeeds.
        self.path = Some(resolved);
        self.baseline = Some(self.text.as_bytes().to_vec());
        Ok(())
    }
    pub fn unchanged_on_disk(&self) -> bool {
        self.path.as_ref().and_then(|p| fs::read(p).ok()).as_ref() == self.baseline.as_ref()
            && self.path.is_some()
    }
}

/// A bounded project inventory; never descends into build output or symlink directories.
pub fn scripts(root: &Path) -> Vec<PathBuf> {
    fn visit(root: &Path, out: &mut Vec<PathBuf>, depth: usize) {
        if depth > 12 || out.len() >= 500 {
            return;
        }
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            if out.len() >= 500 {
                break;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() {
                if ![".git", "target", "dist", "node_modules", ".codex"]
                    .contains(&entry.file_name().to_string_lossy().as_ref())
                {
                    visit(&path, out, depth + 1);
                }
            } else if path.extension().is_some_and(|e| e == "camel") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    visit(root, &mut out, 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saves_commit_and_repeat_safely() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.camel");
        let mut d = Document::new("workflow void Main() {}".into());
        assert!(d.dirty());
        d.save(&path).unwrap();
        assert!(!d.dirty());
        d.save(&path).unwrap();
        d.text.push('\n');
        assert!(d.dirty());
        d.save(&path).unwrap();
        assert!(d.unchanged_on_disk());
    }
    #[test]
    fn external_changes_and_save_as_collisions_preserve_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.camel");
        fs::write(&path, "original").unwrap();
        let mut d = Document::open(&path).unwrap();
        d.text = "edited".into();
        fs::write(&path, "external").unwrap();
        assert!(d.save(&path).is_err());
        assert!(d.dirty());
        assert_eq!(fs::read_to_string(&path).unwrap(), "external");
        let other = dir.path().join("b.camel");
        fs::write(&other, "other").unwrap();
        assert!(d.save(&other).is_err());
        assert_eq!(d.text, "edited");
        let new = dir.path().join("c.camel");
        d.save(&new).unwrap();
        assert!(!d.dirty());
    }
    #[test]
    fn inventory_ignores_distribution_and_build_copies() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("dist")).unwrap();
        fs::write(dir.path().join("dist/copy.camel"), "").unwrap();
        fs::write(dir.path().join("main.camel"), "").unwrap();
        assert_eq!(scripts(dir.path()), vec![dir.path().join("main.camel")]);
    }
}
