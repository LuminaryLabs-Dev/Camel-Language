use super::syntax::*;
use std::collections::BTreeSet;

pub const BUILTINS: &[&str] = &[
    "log", "env", "record", "audio", "json", "len", "string", "int", "double", "assert", "range",
    "clock",
];
pub const LANGUAGE_VERSION: &str = "0.2";
const KEYWORDS: &[&str] = &[
    "record",
    "agent",
    "policy",
    "tool",
    "workflow",
    "using",
    "import",
    "if",
    "else",
    "while",
    "for",
    "foreach",
    "in",
    "return",
    "break",
    "continue",
    "throw",
    "try",
    "catch",
    "finally",
    "var",
    "new",
    "spawn",
    "await",
    "true",
    "false",
    "null",
    "version",
    "checkpoint",
];
fn binding_name(name: &str, site: &Site) -> Result<()> {
    if KEYWORDS.contains(&name) {
        Err(Error::at(site, format!("'{name}' is a reserved keyword")))
    } else {
        Ok(())
    }
}

fn type_error(site: &Site, message: impl Into<String>) -> Error {
    let mut error = Error::at(site, message);
    error.phase = DiagnosticPhase::Type;
    error
}

fn validate_checkpoints(function: &Function) -> Result<()> {
    fn nested(statement: &Stmt) -> Result<()> {
        match &statement.kind {
            StmtKind::Checkpoint(_) => Err(Error::at(
                &statement.site,
                "checkpoint must be a direct statement in a workflow body",
            )),
            StmtKind::Block(items) => items.iter().try_for_each(nested),
            StmtKind::If(_, yes, no) => {
                nested(yes)?;
                if let Some(no) = no {
                    nested(no)?;
                }
                Ok(())
            }
            StmtKind::While(_, body) | StmtKind::Each(_, _, _, body) => nested(body),
            StmtKind::For(init, _, step, body) => {
                nested(init)?;
                nested(step)?;
                nested(body)
            }
            StmtKind::Try(body, catch, finally) => {
                nested(body)?;
                if let Some((_, catch)) = catch {
                    nested(catch)?;
                }
                if let Some(finally) = finally {
                    nested(finally)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    let StmtKind::Block(items) = &function.body.kind else {
        return Ok(());
    };
    let mut labels = BTreeSet::new();
    for item in items {
        if let StmtKind::Checkpoint(label) = &item.kind {
            if !function.workflow {
                return Err(Error::at(
                    &item.site,
                    "checkpoint is only allowed in a workflow",
                ));
            }
            if !labels.insert(label) {
                return Err(Error::at(
                    &item.site,
                    format!("duplicate checkpoint '{label}'"),
                ));
            }
        } else {
            nested(item)?;
        }
    }
    Ok(())
}

/// Collect independent declaration-level diagnostics without stopping at the
/// first invalid declaration. A declaration still stops at its first error
/// to avoid cascades; later declarations are checked against the full program.
pub(super) fn check_diagnostics(p: &Program) -> Vec<(Error, &'static str)> {
    const CHECK_CODE: &str = "CAMEL-CHECK-001";
    const TYPE_CODE: &str = "CAMEL-TYPE-001";
    let mut diagnostics = Vec::new();
    if let Some(version) = &p.language_version {
        if version != LANGUAGE_VERSION {
            diagnostics.push((Error::at(
                p.sites.values().next().unwrap_or(&Site {
                    file: "<source>".into(),
                    line: 1,
                    column: 1,
                    offset: 0,
                    end_line: 1,
                    end_column: 1,
                    end_offset: 0,
                }),
                format!(
                    "unsupported Camel language version '{version}'; this runtime supports '{LANGUAGE_VERSION}'"
                ),
            ), CHECK_CODE));
            return diagnostics;
        }
    }
    for (name, site) in &p.sites {
        if let Err(error) = binding_name(name, site) {
            diagnostics.push((error, CHECK_CODE));
            continue;
        }
        if BUILTINS.contains(&name.as_str())
            || [
                "void",
                "bool",
                "any",
                "object",
                "Audio",
                "AgentResult",
                "EvidenceProvenance",
                "Agent",
                "Function",
                "Task",
                "Tool",
            ]
            .contains(&name.as_str())
        {
            diagnostics.push((Error::at(site, format!("'{name}' is reserved")), CHECK_CODE));
        }
    }
    let global: BTreeSet<String> = p
        .sites
        .keys()
        .cloned()
        .chain(BUILTINS.iter().map(|s| s.to_string()))
        .collect();
    let validator = Checker { p, global };
    for (name, fields) in &p.records {
        for (field, t) in fields {
            if let Err(error) = binding_name(field, &p.sites[name]) {
                diagnostics.push((error, CHECK_CODE));
                continue;
            }
            if *t == Type::named("void") {
                diagnostics.push((
                    type_error(&p.sites[name], "record field cannot be void"),
                    CHECK_CODE,
                ));
                continue;
            }
            if let Err(error) = validator.ty(t, &p.sites[name]) {
                diagnostics.push((error, CHECK_CODE));
            }
        }
    }
    for f in p.functions.values() {
        let checked = (|| {
            validator.ty(&f.returns, &f.site)?;
            let locals = validator.params(&f.params, &f.site)?;
            validator.stmt(&f.body, &mut vec![locals], 0, &f.returns)
        })();
        if let Err(error) = checked {
            diagnostics.push((error, CHECK_CODE));
        } else {
            if let Err(error) = validate_checkpoints(f) {
                diagnostics.push((error, CHECK_CODE));
            }
            diagnostics.extend(
                super::static_types::check_function_diagnostics(p, f)
                    .into_iter()
                    .map(|error| (error, TYPE_CODE)),
            );
        }
    }
    for (name, c) in &p.agents {
        let checked = (|| {
            validator.config(
                c,
                &[
                    "model",
                    "system",
                    "tools",
                    "policy",
                    "response",
                    "maxTurns",
                    "retries",
                    "temperature",
                ],
                &p.sites[name],
            )?;
            if !c.contains_key("model") {
                return Err(Error::at(&p.sites[name], "agent requires model"));
            }
            if let Some(e) = c.get("response") {
                match &e.kind {
                    ExprKind::Name(n) if p.records.contains_key(n) => {}
                    _ => return Err(Error::at(&e.site, "response must name a record")),
                }
            }
            if let Some(e) = c.get("policy") {
                match &e.kind {
                    ExprKind::Name(n) if p.policies.contains_key(n) => {}
                    _ => return Err(Error::at(&e.site, "policy must name a policy")),
                }
            }
            if let Some(e) = c.get("tools") {
                validator.tool_list(e)?;
            }
            Ok(())
        })();
        if let Err(error) = checked {
            diagnostics.push((error, CHECK_CODE));
        }
    }
    for (name, c) in &p.policies {
        let checked = (|| {
            validator.config(
                c,
                &[
                    "allowTools",
                    "maxToolCalls",
                    "allowNetwork",
                    "allowFileRead",
                    "allowFileWrite",
                    "allowShell",
                ],
                &p.sites[name],
            )?;
            if let Some(e) = c.get("allowTools") {
                validator.tool_list(e)?;
            }
            Ok(())
        })();
        if let Err(error) = checked {
            diagnostics.push((error, CHECK_CODE));
        }
    }
    for t in p.tools.values() {
        let checked = (|| {
            validator.ty(&t.returns, &t.site)?;
            validator.params(&t.params, &t.site)?;
            validator.config(&t.config, &["handler", "description"], &t.site)?;
            if !t.config.contains_key("handler") {
                return Err(Error::at(&t.site, "tool requires handler"));
            }
            Ok(())
        })();
        if let Err(error) = checked {
            diagnostics.push((error, CHECK_CODE));
        } else if let Err(error) = super::static_types::check_tool(p, t) {
            diagnostics.push((error, TYPE_CODE));
        }
    }
    diagnostics.sort_by(|(left, _), (right, _)| {
        left.site
            .file
            .cmp(&right.site.file)
            .then(left.site.line.cmp(&right.site.line))
            .then(left.site.column.cmp(&right.site.column))
    });
    diagnostics
}
struct Checker<'a> {
    p: &'a Program,
    global: BTreeSet<String>,
}
impl Checker<'_> {
    fn ty(&self, t: &Type, site: &Site) -> Result<()> {
        match t {
            Type::Task(t) => self.ty(t, site),
            Type::Array(t) => {
                if *t.as_ref() == Type::named("void") {
                    return Err(type_error(site, "void cannot be an element type"));
                }
                self.ty(t, site)
            }
            Type::Named(n)
                if [
                    "int",
                    "double",
                    "bool",
                    "string",
                    "void",
                    "any",
                    "object",
                    "Audio",
                    "AgentResult",
                    "EvidenceProvenance",
                    "Agent",
                    "Function",
                    "Tool",
                ]
                .contains(&n.as_str())
                    || self.p.records.contains_key(n) =>
            {
                Ok(())
            }
            _ => Err(type_error(site, format!("unknown type '{t}'"))),
        }
    }
    fn params(&self, params: &[Param], site: &Site) -> Result<BTreeSet<String>> {
        let mut scope = BTreeSet::new();
        let mut default_seen = false;
        for p in params {
            binding_name(&p.name, site)?;
            self.ty(&p.ty, site)?;
            if p.ty == Type::named("void") {
                return Err(type_error(site, "parameter cannot be void"));
            }
            if !scope.insert(p.name.clone()) {
                return Err(Error::at(site, format!("duplicate parameter '{}'", p.name)));
            }
            if let Some(e) = &p.default {
                self.expr(e, &[BTreeSet::new()])?;
                default_seen = true;
            } else if default_seen {
                return Err(Error::at(
                    site,
                    "required parameter follows default parameter",
                ));
            }
        }
        Ok(scope)
    }
    fn config(
        &self,
        c: &std::collections::BTreeMap<String, Expr>,
        allowed: &[&str],
        site: &Site,
    ) -> Result<()> {
        for (k, e) in c {
            if !allowed.contains(&k.as_str()) {
                return Err(Error::at(
                    site,
                    format!("unknown configuration field '{k}'"),
                ));
            }
            self.expr(e, &[])?;
        }
        Ok(())
    }
    fn tool_list(&self, e: &Expr) -> Result<()> {
        if let ExprKind::Array(items) = &e.kind {
            for item in items {
                match &item.kind {
                    ExprKind::Name(n) if self.p.tools.contains_key(n) => {}
                    _ => {
                        return Err(Error::at(
                            &item.site,
                            "tool lists require declared tool names",
                        ))
                    }
                }
            }
            Ok(())
        } else {
            Err(Error::at(&e.site, "expected tool list"))
        }
    }
    fn expr(&self, e: &Expr, scopes: &[BTreeSet<String>]) -> Result<()> {
        match &e.kind {
            ExprKind::Name(n)
                if !self.global.contains(n) && !scopes.iter().rev().any(|s| s.contains(n)) =>
            {
                return Err(Error::at(&e.site, format!("unknown name '{n}'")))
            }
            ExprKind::Array(items) | ExprKind::Interpolate(items) => {
                for e in items {
                    self.expr(e, scopes)?;
                }
            }
            ExprKind::Object(ty, fields) => {
                if let Some(n) = ty {
                    let declared = self
                        .p
                        .records
                        .get(n)
                        .ok_or_else(|| Error::at(&e.site, format!("unknown record '{n}'")))?;
                    let supplied: BTreeSet<_> = fields.iter().map(|(k, _)| k).collect();
                    if supplied.into_iter().ne(declared.keys()) {
                        return Err(Error::at(
                            &e.site,
                            format!("'{n}' requires exactly its declared fields"),
                        ));
                    }
                }
                for (_, e) in fields {
                    self.expr(e, scopes)?;
                }
            }
            ExprKind::Unary(_, e) | ExprKind::Member(e, _) => self.expr(e, scopes)?,
            ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => {
                self.expr(a, scopes)?;
                self.expr(b, scopes)?;
            }
            ExprKind::Call(f, args) => {
                self.expr(f, scopes)?;
                for e in args {
                    self.expr(e, scopes)?;
                }
                if let ExprKind::Name(n) = &f.kind {
                    if !scopes.iter().any(|s| s.contains(n)) {
                        if let Some(fun) = self.p.functions.get(n) {
                            arity(&fun.params, args.len(), &e.site)?;
                        }
                        if let Some(tool) = self.p.tools.get(n) {
                            arity(&tool.params, args.len(), &e.site)?;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn stmt(
        &self,
        s: &Stmt,
        scopes: &mut Vec<BTreeSet<String>>,
        loops: usize,
        returns: &Type,
    ) -> Result<()> {
        match &s.kind {
            StmtKind::Block(items) => {
                scopes.push(BTreeSet::new());
                for s in items {
                    self.stmt(s, scopes, loops, returns)?;
                }
                scopes.pop();
            }
            StmtKind::Let(n, t, e) => {
                binding_name(n, &s.site)?;
                self.expr(e, scopes)?;
                if let Some(t) = t {
                    self.ty(t, &s.site)?;
                    if *t == Type::named("void") {
                        return Err(type_error(&s.site, "variable cannot be void"));
                    }
                }
                if !scopes.last_mut().unwrap().insert(n.clone()) {
                    return Err(Error::at(&s.site, format!("duplicate variable '{n}'")));
                }
            }
            StmtKind::Assign(a, b) | StmtKind::Update(a, _, b) => {
                self.expr(a, scopes)?;
                self.expr(b, scopes)?;
                let mut root = a;
                while let ExprKind::Member(e, _) | ExprKind::Index(e, _) = &root.kind {
                    root = e;
                }
                if !matches!(&root.kind,ExprKind::Name(n) if scopes.iter().any(|s|s.contains(n))) {
                    return Err(Error::at(&s.site, "assignment requires a local variable"));
                }
            }
            StmtKind::Expr(e) | StmtKind::Throw(e) => self.expr(e, scopes)?,
            StmtKind::Checkpoint(_) => {}
            StmtKind::Return(e) => {
                if let Some(e) = e {
                    if *returns == Type::named("void") {
                        return Err(Error::at(&s.site, "void function cannot return a value"));
                    }
                    self.expr(e, scopes)?;
                } else if *returns != Type::named("void") {
                    return Err(Error::at(
                        &s.site,
                        "non-void function requires a return value",
                    ));
                }
            }
            StmtKind::If(c, a, b) => {
                self.expr(c, scopes)?;
                self.branch(a, scopes, loops, returns)?;
                if let Some(b) = b {
                    self.branch(b, scopes, loops, returns)?;
                }
            }
            StmtKind::While(c, b) => {
                self.expr(c, scopes)?;
                self.branch(b, scopes, loops + 1, returns)?;
            }
            StmtKind::For(init, c, step, b) => {
                scopes.push(BTreeSet::new());
                self.stmt(init, scopes, loops, returns)?;
                self.expr(c, scopes)?;
                self.stmt(step, scopes, loops + 1, returns)?;
                self.branch(b, scopes, loops + 1, returns)?;
                scopes.pop();
            }
            StmtKind::Each(n, t, e, b) => {
                binding_name(n, &s.site)?;
                self.expr(e, scopes)?;
                if let Some(t) = t {
                    self.ty(t, &s.site)?;
                }
                scopes.push([n.clone()].into_iter().collect());
                self.branch(b, scopes, loops + 1, returns)?;
                scopes.pop();
            }
            StmtKind::Break | StmtKind::Continue if loops == 0 => {
                return Err(Error::at(&s.site, "loop control outside a loop"))
            }
            StmtKind::Try(b, c, f) => {
                self.branch(b, scopes, loops, returns)?;
                if let Some((n, b)) = c {
                    binding_name(n, &s.site)?;
                    scopes.push([n.clone()].into_iter().collect());
                    self.branch(b, scopes, loops, returns)?;
                    scopes.pop();
                }
                if let Some(b) = f {
                    self.branch(b, scopes, loops, returns)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn branch(
        &self,
        s: &Stmt,
        scopes: &mut Vec<BTreeSet<String>>,
        loops: usize,
        returns: &Type,
    ) -> Result<()> {
        scopes.push(BTreeSet::new());
        let r = self.stmt(s, scopes, loops, returns);
        scopes.pop();
        r
    }
}
pub fn arity(params: &[Param], count: usize, site: &Site) -> Result<()> {
    let required = params.iter().filter(|p| p.default.is_none()).count();
    if count < required || count > params.len() {
        Err(Error::at(
            site,
            format!(
                "expected {required}..{} arguments, got {count}",
                params.len()
            ),
        ))
    } else {
        Ok(())
    }
}
