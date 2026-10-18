pub mod bundle;
pub mod install;
mod path;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub(crate) fn canonicalize(path: &std::path::Path) -> Result<std::path::PathBuf> {
    let canonical = path.canonicalize()?;
    #[cfg(windows)]
    {
        let value = canonical.to_string_lossy();
        if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
            return Ok(std::path::PathBuf::from(format!(r"\\{unc}")));
        }
        if let Some(normal) = value.strip_prefix(r"\\?\") {
            return Ok(normal.into());
        }
    }
    Ok(canonical)
}
