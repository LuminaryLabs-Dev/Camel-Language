use super::{syntax::*, CompiledProgram};

#[derive(Clone, Debug)]
pub struct Parameter {
    pub name: String,
    pub ty: String,
    pub optional: bool,
    /// Source-like text, not an evaluated expression.
    pub default: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Declaration {
    pub name: String,
    pub kind: String,
    pub site: Site,
    pub parameters: Vec<Parameter>,
    pub details: Vec<(String, String)>,
}
impl CompiledProgram {
    /// Pure AST metadata: never evaluates configuration, defaults, or user functions.
    pub fn declarations(&self) -> Vec<Declaration> {
        let p = &self.program;
        let mut out = Vec::new();
        for (name, site) in &p.sites {
            let mut d = Declaration {
                name: name.clone(),
                kind: String::new(),
                site: site.clone(),
                parameters: Vec::new(),
                details: Vec::new(),
            };
            if let Some(f) = p.functions.get(name) {
                d.kind = if f.workflow { "workflow" } else { "function" }.into();
                d.parameters = parameters(&f.params);
                d.details.push(("returns".into(), f.returns.to_string()));
            } else if let Some(t) = p.tools.get(name) {
                d.kind = "tool".into();
                d.parameters = parameters(&t.params);
                d.details = config(&t.config);
                d.details.push(("returns".into(), t.returns.to_string()));
            } else if let Some(c) = p.agents.get(name) {
                d.kind = "agent".into();
                d.details = config(c);
            } else if let Some(c) = p.policies.get(name) {
                d.kind = "policy".into();
                d.details = config(c);
            } else if let Some(r) = p.records.get(name) {
                d.kind = "record".into();
                d.details = r.iter().map(|(n, t)| (n.clone(), t.to_string())).collect();
            }
            out.push(d);
        }
        out.sort_by(|a, b| {
            a.site
                .file
                .cmp(&b.site.file)
                .then(a.site.line.cmp(&b.site.line))
        });
        out
    }
}
fn parameters(p: &[Param]) -> Vec<Parameter> {
    p.iter()
        .map(|p| Parameter {
            name: p.name.clone(),
            ty: p.ty.to_string(),
            optional: p.default.is_some(),
            default: p.default.as_ref().map(expression),
        })
        .collect()
}
fn config(c: &std::collections::BTreeMap<String, Expr>) -> Vec<(String, String)> {
    c.iter().map(|(k, e)| (k.clone(), expression(e))).collect()
}
fn expression(e: &Expr) -> String {
    match &e.kind {
        ExprKind::String(s) => serde_json::to_string(s).unwrap(),
        ExprKind::Name(n) => n.clone(),
        ExprKind::Null => "null".into(),
        ExprKind::Bool(b) => b.to_string(),
        ExprKind::Int(n) => n.to_string(),
        ExprKind::Float(n) => n.to_string(),
        ExprKind::Array(a) => format!(
            "[{}]",
            a.iter().map(expression).collect::<Vec<_>>().join(", ")
        ),
        ExprKind::Call(f, a) => format!(
            "{}({})",
            expression(f),
            a.iter().map(expression).collect::<Vec<_>>().join(", ")
        ),
        _ => "<expression; evaluated only when run>".into(),
    }
}
