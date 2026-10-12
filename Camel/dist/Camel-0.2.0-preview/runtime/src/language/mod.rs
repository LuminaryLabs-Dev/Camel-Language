mod agents;
mod check;
mod eval;
mod host;
mod lexer;
mod parser;
mod syntax;
mod value;

pub use eval::{Engine, Execution, Limits};
pub use host::{Capability, Host, ModelProvider, ToolRegistry};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
};
pub use syntax::{Error, Result, Site, Type};
pub use value::Value;

#[derive(Clone)]
pub struct CompiledProgram {
    pub(crate) program: Arc<syntax::Program>,
    pub(crate) root: PathBuf,
}
impl CompiledProgram {
    pub fn from_source(name: &str, source: &str) -> Result<Self> {
        let p = parser::parse(name, source)?;
        if !p.imports.is_empty() {
            return Err(Error::at(&p.imports[0].1, "use load() to resolve imports"));
        }
        check::check(&p)?;
        Ok(Self {
            program: Arc::new(p),
            root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        })
    }
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let mut loaded = BTreeSet::new();
        let mut active = BTreeSet::new();
        let mut merged = syntax::Program::default();
        let site = Site {
            file: path.as_ref().display().to_string(),
            line: 1,
            column: 1,
        };
        let entry = path
            .as_ref()
            .canonicalize()
            .map_err(|e| Error::at(&site, e.to_string()))?;
        load_module(&entry, &mut loaded, &mut active, &mut merged, &site)?;
        check::check(&merged)?;
        Ok(Self {
            program: Arc::new(merged),
            root: entry.parent().unwrap().to_path_buf(),
        })
    }
    pub fn entry_parameters(&self, name: &str) -> Result<Vec<(String, Type)>> {
        let f = self.program.functions.get(name).ok_or_else(|| {
            Error::at(
                &Site {
                    file: self.root.display().to_string(),
                    line: 1,
                    column: 1,
                },
                format!("missing workflow '{name}'"),
            )
        })?;
        if !f.workflow {
            return Err(Error::at(&f.site, "entry must be a workflow"));
        }
        Ok(f.params
            .iter()
            .map(|p| (p.name.clone(), p.ty.clone()))
            .collect())
    }
}
fn load_module(
    path: &Path,
    loaded: &mut BTreeSet<PathBuf>,
    active: &mut BTreeSet<PathBuf>,
    merged: &mut syntax::Program,
    site: &Site,
) -> Result<()> {
    if active.len() >= 128 {
        return Err(Error::at(site, "import nesting exceeds 128"));
    }
    if active.contains(path) {
        return Err(Error::at(
            site,
            format!("cyclic import: {}", path.display()),
        ));
    }
    if loaded.contains(path) {
        return Ok(());
    }
    active.insert(path.into());
    let text = std::fs::read_to_string(path).map_err(|e| Error::at(site, e.to_string()))?;
    let p = parser::parse(&path.display().to_string(), &text)?;
    for (relative, site) in &p.imports {
        let relative = Path::new(relative);
        if relative.is_absolute() || relative.extension().is_none_or(|e| e != "camel") {
            return Err(Error::at(site, "import requires a relative .camel path"));
        }
        let target = path
            .parent()
            .unwrap()
            .join(relative)
            .canonicalize()
            .map_err(|e| Error::at(site, e.to_string()))?;
        load_module(&target, loaded, active, merged, site)?;
    }
    for (name, site) in &p.sites {
        if merged.sites.contains_key(name) {
            return Err(Error::at(
                site,
                format!("duplicate imported declaration '{name}'"),
            ));
        }
    }
    merged.sites.extend(p.sites);
    merged.functions.extend(p.functions);
    merged.records.extend(p.records);
    merged.agents.extend(p.agents);
    merged.policies.extend(p.policies);
    merged.tools.extend(p.tools);
    active.remove(path);
    loaded.insert(path.into());
    Ok(())
}
