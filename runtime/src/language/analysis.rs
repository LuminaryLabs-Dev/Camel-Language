use super::{CompiledProgram, Declaration, Diagnostic, Error, Result, Site};
use std::path::Path;

/// Results of analyzing source without running it.
pub struct AnalysisResult {
    program: Option<CompiledProgram>,
    declarations: Vec<Declaration>,
    diagnostics: Vec<Diagnostic>,
}

impl AnalysisResult {
    fn from_result(result: std::result::Result<(CompiledProgram, Vec<Diagnostic>), Vec<Diagnostic>>) -> Self {
        match result {
            Ok((program, diagnostics)) => Self {
                declarations: program.declarations(),
                program: Some(program),
                diagnostics,
            },
            Err(diagnostics) => Self {
                program: None,
                declarations: Vec::new(),
                diagnostics,
            },
        }
    }

    pub fn program(&self) -> Option<&CompiledProgram> {
        self.program.as_ref()
    }

    pub fn declarations(&self) -> &[Declaration] {
        &self.declarations
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn is_valid(&self) -> bool {
        self.program.is_some() && !self.diagnostics.iter().any(|d| d.severity == super::Severity::Error)
    }

    pub fn into_program(self) -> Result<CompiledProgram> {
        let error = self.diagnostics.into_iter().find(|d| d.severity == super::Severity::Error);
        match (self.program, error) {
            (Some(program), _) => Ok(program),
            (None, Some(diagnostic)) => Err(diagnostic.to_error()),
            (None, None) => Err(Error::at(
                &Site {
                    file: "<analysis>".into(),
                    line: 1,
                    column: 1,
                    offset: 0,
                    end_line: 1,
                    end_column: 1,
                    end_offset: 0,
                },
                "analysis produced neither a program nor a diagnostic",
            )),
        }
    }
}

/// Analyze a saved Camel file through the shared runtime front end.
pub fn analyze_file(path: impl AsRef<Path>) -> AnalysisResult {
    AnalysisResult::from_result(CompiledProgram::compile_path_with_diagnostics(path, None))
}

/// Analyze source text that has no imports and no associated filesystem path.
pub fn analyze_source(name: &str, source: &str) -> AnalysisResult {
    AnalysisResult::from_result(CompiledProgram::compile_source_with_diagnostics(
        name, source,
    ))
}

/// Analyze an unsaved entry buffer, resolving imports beside its file path.
pub fn analyze_buffer(path: impl AsRef<Path>, source: &str) -> AnalysisResult {
    AnalysisResult::from_result(CompiledProgram::compile_path_with_diagnostics(
        path,
        Some(source),
    ))
}
