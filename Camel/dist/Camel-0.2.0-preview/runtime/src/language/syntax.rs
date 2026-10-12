use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct Site {
    pub file: String,
    pub line: usize,
    pub column: usize,
}
#[derive(Clone, Debug)]
pub struct Error {
    pub message: String,
    pub site: Site,
    pub fatal: bool,
}
impl Error {
    pub fn at(site: &Site, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            site: site.clone(),
            fatal: false,
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

#[derive(Clone, Debug, PartialEq, Eq)]
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
    pub functions: BTreeMap<String, Function>,
    pub records: BTreeMap<String, BTreeMap<String, Type>>,
    pub agents: BTreeMap<String, BTreeMap<String, Expr>>,
    pub policies: BTreeMap<String, BTreeMap<String, Expr>>,
    pub tools: BTreeMap<String, Tool>,
    pub imports: Vec<(String, Site)>,
    pub sites: BTreeMap<String, Site>,
}
