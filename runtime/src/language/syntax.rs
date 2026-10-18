use std::collections::BTreeMap;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Site {
    pub file: String,
    pub line: usize,
    pub column: usize,
    /// UTF-8 byte offset of the token start.
    #[serde(default)]
    pub offset: usize,
    /// End position of the token, exclusive. Legacy sites use zero here.
    #[serde(default)]
    pub end_line: usize,
    #[serde(default)]
    pub end_column: usize,
    #[serde(default)]
    pub end_offset: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Severity {
    Error,
    Warning,
    Information,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DiagnosticPhase {
    Syntax,
    #[default]
    Analysis,
    Type,
    Lint,
    Runtime,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SourceSpan {
    /// Source positions are one-based; offsets are UTF-8 byte indices.
    pub start: Site,
    pub end: Site,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RelatedInformation {
    pub span: SourceSpan,
    pub message: String,
}

/// Stable, editor-friendly diagnostic returned by the shared analysis API.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    #[serde(default)]
    pub phase: DiagnosticPhase,
    pub code: String,
    pub message: String,
    pub primary_span: SourceSpan,
    pub related: Vec<RelatedInformation>,
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn from_error(code: impl Into<String>, phase: DiagnosticPhase, error: Error) -> Self {
        let site = error.site;
        let end = Site {
            file: site.file.clone(),
            line: if site.end_line == 0 {
                site.line
            } else {
                site.end_line
            },
            column: if site.end_column == 0 {
                site.column
            } else {
                site.end_column
            },
            offset: site.end_offset.max(site.offset),
            end_line: if site.end_line == 0 {
                site.line
            } else {
                site.end_line
            },
            end_column: if site.end_column == 0 {
                site.column
            } else {
                site.end_column
            },
            end_offset: site.end_offset.max(site.offset),
        };
        Self {
            severity: Severity::Error,
            phase,
            code: code.into(),
            message: error.message,
            primary_span: SourceSpan { start: site, end },
            related: Vec::new(),
            help: None,
        }
    }

    pub fn warning(
        code: impl Into<String>,
        phase: DiagnosticPhase,
        site: Site,
        message: impl Into<String>,
    ) -> Self {
        let end = Site {
            file: site.file.clone(),
            line: if site.end_line == 0 {
                site.line
            } else {
                site.end_line
            },
            column: if site.end_column == 0 {
                site.column
            } else {
                site.end_column
            },
            offset: site.end_offset.max(site.offset),
            end_line: if site.end_line == 0 {
                site.line
            } else {
                site.end_line
            },
            end_column: if site.end_column == 0 {
                site.column
            } else {
                site.end_column
            },
            end_offset: site.end_offset.max(site.offset),
        };
        Self {
            severity: Severity::Warning,
            phase,
            code: code.into(),
            message: message.into(),
            primary_span: SourceSpan { start: site, end },
            related: Vec::new(),
            help: None,
        }
    }

    pub fn to_error(&self) -> Error {
        Error::at(&self.primary_span.start, self.message.clone())
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let site = &self.primary_span.start;
        write!(
            f,
            "{}:{}:{}: [{:?} {:?} {}] {}",
            site.file, site.line, site.column, self.severity, self.phase, self.code, self.message
        )
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Error {
    pub message: String,
    pub site: Site,
    pub fatal: bool,
    #[serde(default)]
    pub phase: DiagnosticPhase,
}
impl Error {
    pub fn at(site: &Site, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            site: site.clone(),
            fatal: false,
            phase: DiagnosticPhase::Analysis,
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}",
            self.site.file, self.site.line, self.site.column, self.message
        )
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Type {
    Named(String),
    Array(Box<Type>),
    Task(Box<Type>),
}
impl Type {
    pub fn named(name: &str) -> Self {
        Self::Named(name.into())
    }
}
impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Named(n) => write!(f, "{n}"),
            Self::Array(t) => write!(f, "{t}[]"),
            Self::Task(t) => write!(f, "Task<{t}>"),
        }
    }
}
#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub site: Site,
}
#[derive(Clone, Debug)]
pub enum ExprKind {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Interpolate(Vec<Expr>),
    Name(String),
    Array(Vec<Expr>),
    Object(Option<String>, Vec<(String, Expr)>),
    Unary(String, Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
    Member(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
}
#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub site: Site,
}
#[derive(Clone, Debug)]
pub enum StmtKind {
    Block(Vec<Stmt>),
    Let(String, Option<Type>, Expr),
    Assign(Expr, Expr),
    Update(Expr, String, Expr),
    Expr(Expr),
    /// A durable, explicitly declared workflow boundary.
    Checkpoint(String),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    While(Expr, Box<Stmt>),
    For(Box<Stmt>, Expr, Box<Stmt>, Box<Stmt>),
    Each(String, Option<Type>, Expr, Box<Stmt>),
    Return(Option<Expr>),
    Break,
    Continue,
    Throw(Expr),
    Try(Box<Stmt>, Option<(String, Box<Stmt>)>, Option<Box<Stmt>>),
}
#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub site: Site,
    pub default: Option<Expr>,
}
#[derive(Clone, Debug)]
pub struct Function {
    pub params: Vec<Param>,
    pub returns: Type,
    pub body: Stmt,
    pub workflow: bool,
    pub site: Site,
}
#[derive(Clone, Debug)]
pub struct Tool {
    pub params: Vec<Param>,
    pub returns: Type,
    pub config: BTreeMap<String, Expr>,
    pub site: Site,
}
#[derive(Clone, Debug, Default)]
pub struct Program {
    pub language_version: Option<String>,
    pub functions: BTreeMap<String, Function>,
    pub records: BTreeMap<String, BTreeMap<String, Type>>,
    pub agents: BTreeMap<String, BTreeMap<String, Expr>>,
    pub policies: BTreeMap<String, BTreeMap<String, Expr>>,
    pub tools: BTreeMap<String, Tool>,
    pub imports: Vec<(String, Site)>,
    pub imported_names: std::collections::BTreeSet<String>,
    pub sites: BTreeMap<String, Site>,
}
