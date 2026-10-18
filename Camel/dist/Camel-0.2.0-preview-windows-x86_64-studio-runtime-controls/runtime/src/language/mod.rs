mod agents;
mod analysis;
mod check;
mod eval;
mod host;
mod inspect;
mod lexer;
mod lint;
mod parser;
mod static_types;
mod syntax;
mod value;

pub use analysis::{analyze_buffer, analyze_file, analyze_source, AnalysisResult};
pub use eval::{Engine, Execution, Limits};
pub use host::{
    CancellationToken, Capability, ExecutionEvent, Host, ModelProvider, ProviderCapabilities,
    ToolRegistry,
};
pub use inspect::{Declaration, Parameter};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
};
pub use syntax::{
    Diagnostic, DiagnosticPhase, Error, RelatedInformation, Result, Severity, Site, SourceSpan,
    Type,
};
pub use value::Value;

/// Maximum UTF-8 source bytes accepted for one Camel file.
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone)]
pub struct CompiledProgram {
    pub(crate) program: Arc<syntax::Program>,
    pub(crate) root: PathBuf,
}
impl CompiledProgram {
    pub fn from_source(name: &str, source: &str) -> Result<Self> {
        analyze_source(name, source).into_program()
    }
    pub(super) fn compile_source_with_diagnostics(
        name: &str,
        source: &str,
    ) -> std::result::Result<(Self, Vec<Diagnostic>), Vec<Diagnostic>> {
        let p = parser::parse(name, source).map_err(|error| {
            vec![Diagnostic::from_error(
                "CAMEL-SYNTAX-001",
                DiagnosticPhase::Syntax,
                error,
            )]
        })?;
        if !p.imports.is_empty() {
            return Err(vec![Diagnostic::from_error(
                "CAMEL-IMPORT-001",
                DiagnosticPhase::Analysis,
                Error::at(&p.imports[0].1, "use load() to resolve imports"),
            )]);
        }
        let mut diagnostics = checked_diagnostics(&p);
        if diagnostics.is_empty() {
            diagnostics.extend(lint::analyze(&p));
        }
        if diagnostics.iter().any(|d| d.severity == Severity::Error) {
            return Err(diagnostics);
        }
        Ok((
            Self {
                program: Arc::new(p),
                root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            },
            diagnostics,
        ))
    }
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        analyze_file(path).into_program()
    }
    /// Check an editor buffer without saving or executing it. Imports resolve beside its path.
    pub fn from_source_at(path: impl AsRef<Path>, source: &str) -> Result<Self> {
        analyze_buffer(path, source).into_program()
    }
    pub(super) fn compile_path_with_diagnostics(
        path: impl AsRef<Path>,
        source: Option<&str>,
    ) -> std::result::Result<(Self, Vec<Diagnostic>), Vec<Diagnostic>> {
        let path = path.as_ref();
        let site = Site {
            file: path.display().to_string(),
            line: 1,
            column: 1,
            offset: 0,
            end_line: 1,
            end_column: 1,
            end_offset: 0,
        };
        let entry = if source.is_some() {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
                .canonicalize()
                .map_err(|e| {
                    vec![Diagnostic::from_error(
                        "CAMEL-ANALYSIS-001",
                        DiagnosticPhase::Analysis,
                        Error::at(&site, e.to_string()),
                    )]
                })?;
            let entry = parent.join(path.file_name().ok_or_else(|| {
                vec![Diagnostic::from_error(
                    "CAMEL-ANALYSIS-001",
                    DiagnosticPhase::Analysis,
                    Error::at(&site, "missing filename"),
                )]
            })?);
            if entry.exists() {
                entry.canonicalize().map_err(|e| {
                    vec![Diagnostic::from_error(
                        "CAMEL-ANALYSIS-001",
                        DiagnosticPhase::Analysis,
                        Error::at(&site, e.to_string()),
                    )]
                })?
            } else {
                entry
            }
        } else {
            path.canonicalize().map_err(|e| {
                vec![Diagnostic::from_error(
                    "CAMEL-ANALYSIS-001",
                    DiagnosticPhase::Analysis,
                    Error::at(&site, e.to_string()),
                )]
            })?
        };
        let mut merged = syntax::Program::default();
        load_module(
            &entry,
            &mut BTreeSet::new(),
            &mut BTreeSet::new(),
            &mut merged,
            &site,
            source,
        )
        .map_err(|error| {
            let phase = error.phase;
            let code = if phase == DiagnosticPhase::Syntax {
                "CAMEL-SYNTAX-001"
            } else {
                "CAMEL-ANALYSIS-001"
            };
            vec![Diagnostic::from_error(code, phase, error)]
        })?;
        let mut diagnostics = checked_diagnostics(&merged);
        if diagnostics.is_empty() {
            diagnostics.extend(lint::analyze(&merged));
        }
        if diagnostics.iter().any(|d| d.severity == Severity::Error) {
            return Err(diagnostics);
        }
        Ok((
            Self {
                program: Arc::new(merged),
                root: entry.parent().unwrap().to_path_buf(),
            },
            diagnostics,
        ))
    }
    pub fn parse_arguments(&self, entry: &str, raw: &[String]) -> Result<Vec<Value>> {
        let params = self.entry_parameters(entry)?;
        let site = &self.program.functions[entry].site;
        if raw.len() > params.len() {
            return Err(Error::at(site, "too many workflow arguments"));
        }
        raw.iter()
            .zip(params)
            .map(|(raw, (_, ty))| {
                let invalid = |e: String| Error::at(site, format!("invalid {ty} argument: {e}"));
                Ok(match &ty {
                    Type::Named(n) if n == "string" => Value::String(raw.clone()),
                    Type::Named(n) if n == "int" => {
                        Value::Int(raw.parse::<i64>().map_err(|e| invalid(e.to_string()))?)
                    }
                    Type::Named(n) if n == "double" => {
                        Value::Float(raw.parse::<f64>().map_err(|e| invalid(e.to_string()))?)
                    }
                    Type::Named(n) if n == "bool" => {
                        Value::Bool(raw.parse::<bool>().map_err(|e| invalid(e.to_string()))?)
                    }
                    Type::Task(_) => {
                        return Err(Error::at(site, "tasks cannot be command-line arguments"))
                    }
                    _ => Value::from_json(
                        serde_json::from_str(raw).map_err(|e| invalid(e.to_string()))?,
                    ),
                })
            })
            .collect()
    }
    pub fn entry_parameters(&self, name: &str) -> Result<Vec<(String, Type)>> {
        let f = self.program.functions.get(name).ok_or_else(|| {
            Error::at(
                &Site {
                    file: self.root.display().to_string(),
                    line: 1,
                    column: 1,
                    offset: 0,
                    end_line: 1,
                    end_column: 1,
                    end_offset: 0,
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

    pub fn language_version(&self) -> &'static str {
        check::LANGUAGE_VERSION
    }
}

fn checked_diagnostics(program: &syntax::Program) -> Vec<Diagnostic> {
    check::check_diagnostics(program)
        .into_iter()
        .map(|(error, code)| {
            let (code, phase) = if error.phase == DiagnosticPhase::Type {
                ("CAMEL-TYPE-002", DiagnosticPhase::Type)
            } else if code.starts_with("CAMEL-TYPE-") {
                (code, DiagnosticPhase::Type)
            } else {
                (code, DiagnosticPhase::Analysis)
            };
            Diagnostic::from_error(code, phase, error)
        })
        .collect()
}

fn load_module(
    path: &Path,
    loaded: &mut BTreeSet<PathBuf>,
    active: &mut BTreeSet<PathBuf>,
    merged: &mut syntax::Program,
    site: &Site,
    source: Option<&str>,
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
    let imported_module = !active.is_empty();
    active.insert(path.into());
    let text = match source {
        Some(s) => s.to_owned(),
        None => std::fs::read_to_string(path).map_err(|e| Error::at(site, e.to_string()))?,
    };
    let p = parser::parse(&path.display().to_string(), &text)?;
    if let Some(version) = &p.language_version {
        if let Some(existing) = &merged.language_version {
            if existing != version {
                return Err(Error::at(
                    &p.sites.values().next().cloned().unwrap_or_else(|| site.clone()),
                    format!("imported module declares language version '{version}', but the program uses '{existing}'"),
                ));
            }
        } else {
            merged.language_version = Some(version.clone());
        }
    }
    for (relative, site) in &p.imports {
        let relative = Path::new(relative);
        if relative.is_absolute() || relative.extension().is_none_or(|e| e != "camel") {
            return Err(Error::at(site, "import requires a .camel path"));
        }
        let local = path.parent().unwrap().join(relative);
        let target = if local.exists() {
            local
        } else {
            let name = relative.to_string_lossy();
            if !name.starts_with("@camel/") {
                return Err(Error::at(
                    site,
                    format!("module '{}' not found beside source", relative.display()),
                ));
            }
            crate::libraries::resolve_import(&name)
                .map_err(|error| Error::at(site, error.to_string()))?
        }
        .canonicalize()
        .map_err(|e| Error::at(site, e.to_string()))?;
        load_module(&target, loaded, active, merged, site, None)?;
    }
    for (name, site) in &p.sites {
        if merged.sites.contains_key(name) {
            return Err(Error::at(
                site,
                format!("duplicate imported declaration '{name}'"),
            ));
        }
    }
    if imported_module {
        merged.imported_names.extend(p.sites.keys().cloned());
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
