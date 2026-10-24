use super::syntax::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy)]
enum BindingKind {
    Local,
    Parameter,
    Foreach,
    Catch,
}

struct Binding {
    name: String,
    site: Site,
    kind: BindingKind,
    used: bool,
}

#[derive(Default)]
struct Usage {
    bindings: Vec<Binding>,
    scopes: Vec<BTreeMap<String, usize>>,
    globals: BTreeSet<String>,
}

impl Usage {
    fn push_scope(&mut self) {
        self.scopes.push(BTreeMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn bind(&mut self, name: &str, site: &Site, kind: BindingKind) {
        let index = self.bindings.len();
        self.bindings.push(Binding {
            name: name.to_owned(),
            site: site.clone(),
            kind,
            used: false,
        });
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_owned(), index);
        }
    }

    fn reference(&mut self, name: &str) {
        if let Some(index) = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
        {
            self.bindings[index].used = true;
        } else {
            self.globals.insert(name.to_owned());
        }
    }

    fn expression(&mut self, expression: &Expr) {
        match &expression.kind {
            ExprKind::Name(name) => self.reference(name),
            ExprKind::Interpolate(items) | ExprKind::Array(items) => {
                for item in items {
                    self.expression(item);
                }
            }
            ExprKind::Object(record, fields) => {
                if let Some(record) = record {
                    self.reference(record);
                }
                for (_, value) in fields {
                    self.expression(value);
                }
            }
            ExprKind::Unary(_, value) | ExprKind::Member(value, _) => self.expression(value),
            ExprKind::Binary(_, left, right) | ExprKind::Index(left, right) => {
                self.expression(left);
                self.expression(right);
            }
            ExprKind::Call(callee, args) => {
                self.expression(callee);
                for arg in args {
                    self.expression(arg);
                }
            }
            ExprKind::Null
            | ExprKind::Bool(_)
            | ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::String(_) => {}
        }
    }

    fn statement(&mut self, statement: &Stmt) {
        match &statement.kind {
            StmtKind::Block(items) => {
                self.push_scope();
                for item in items {
                    self.statement(item);
                }
                self.pop_scope();
            }
            StmtKind::Let(name, _, value) => {
                self.expression(value);
                self.bind(name, &statement.site, BindingKind::Local);
            }
            StmtKind::Assign(left, right) => {
                self.assignment_target(left);
                self.expression(right);
            }
            StmtKind::Update(left, _, right) => {
                self.expression(left);
                self.expression(right);
            }
            StmtKind::Expr(value) | StmtKind::Throw(value) => self.expression(value),
            StmtKind::If(condition, yes, no) => {
                self.expression(condition);
                self.branch(yes);
                if let Some(no) = no {
                    self.branch(no);
                }
            }
            StmtKind::While(condition, body) => {
                self.expression(condition);
                self.branch(body);
            }
            StmtKind::For(init, condition, step, body) => {
                self.push_scope();
                self.statement(init);
                self.expression(condition);
                self.statement(step);
                self.branch(body);
                self.pop_scope();
            }
            StmtKind::Each(name, _, iterable, body) => {
                self.expression(iterable);
                self.push_scope();
                self.bind(name, &statement.site, BindingKind::Foreach);
                self.branch(body);
                self.pop_scope();
            }
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    self.expression(value);
                }
            }
            StmtKind::Try(body, catch, finally) => {
                self.branch(body);
                if let Some((name, body)) = catch {
                    self.push_scope();
                    self.bind(name, &body.site, BindingKind::Catch);
                    self.branch(body);
                    self.pop_scope();
                }
                if let Some(body) = finally {
                    self.branch(body);
                }
            }
            StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Checkpoint(_) => {}
        }
    }

    fn branch(&mut self, statement: &Stmt) {
        self.push_scope();
        self.statement(statement);
        self.pop_scope();
    }

    fn assignment_target(&mut self, expression: &Expr) {
        match &expression.kind {
            ExprKind::Name(_) => {}
            ExprKind::Member(base, _) => self.expression(base),
            ExprKind::Index(base, index) => {
                self.expression(base);
                self.expression(index);
            }
            _ => self.expression(expression),
        }
    }
}

pub(super) fn analyze(program: &Program) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut global_references = BTreeSet::new();

    for function in program.functions.values() {
        let mut usage = Usage::default();
        usage.push_scope();
        for parameter in &function.params {
            usage.bind(&parameter.name, &parameter.site, BindingKind::Parameter);
            collect_type(&parameter.ty, &mut usage.globals);
            if let Some(default) = &parameter.default {
                usage.expression(default);
            }
        }
        collect_type(&function.returns, &mut usage.globals);
        usage.statement(&function.body);
        global_references.extend(usage.globals);
        for binding in usage.bindings {
            if binding.used {
                continue;
            }
            let (code, description) = match binding.kind {
                BindingKind::Local => ("CAMEL-LINT-001", "local"),
                BindingKind::Parameter => ("CAMEL-LINT-002", "parameter"),
                BindingKind::Foreach => ("CAMEL-LINT-001", "foreach variable"),
                BindingKind::Catch => ("CAMEL-LINT-001", "catch variable"),
            };
            diagnostics.push(Diagnostic::warning(
                code,
                DiagnosticPhase::Lint,
                binding.site,
                format!("{description} '{}' is never used", binding.name),
            ));
        }
    }

    for tool in program.tools.values() {
        collect_type(&tool.returns, &mut global_references);
        for parameter in &tool.params {
            collect_type(&parameter.ty, &mut global_references);
            if let Some(default) = &parameter.default {
                collect_global_expression(default, &mut global_references);
            }
        }
        for value in tool.config.values() {
            collect_global_expression(value, &mut global_references);
        }
    }
    for fields in program.records.values() {
        for ty in fields.values() {
            collect_type(ty, &mut global_references);
        }
    }
    for config in program.agents.values().chain(program.policies.values()) {
        for value in config.values() {
            collect_global_expression(value, &mut global_references);
        }
    }

    for name in &program.imported_names {
        if !global_references.contains(name) {
            if let Some(site) = program.sites.get(name) {
                diagnostics.push(Diagnostic::warning(
                    "CAMEL-LINT-003",
                    DiagnosticPhase::Lint,
                    site.clone(),
                    format!("imported declaration '{name}' is never used"),
                ));
            }
        }
    }
    for (name, site) in &program.sites {
        if !program.imported_names.contains(name)
            && !global_references.contains(name)
            && !program
                .functions
                .get(name)
                .is_some_and(|function| function.workflow)
        {
            diagnostics.push(Diagnostic::warning(
                "CAMEL-LINT-004",
                DiagnosticPhase::Lint,
                site.clone(),
                format!("declaration '{name}' is never used"),
            ));
        }
    }

    diagnostics.sort_by(|left, right| {
        left.primary_span
            .start
            .file
            .cmp(&right.primary_span.start.file)
            .then(
                left.primary_span
                    .start
                    .line
                    .cmp(&right.primary_span.start.line),
            )
            .then(
                left.primary_span
                    .start
                    .column
                    .cmp(&right.primary_span.start.column),
            )
            .then(left.code.cmp(&right.code))
    });
    diagnostics
}

fn collect_global_expression(expression: &Expr, references: &mut BTreeSet<String>) {
    match &expression.kind {
        ExprKind::Name(name) => {
            references.insert(name.clone());
        }
        ExprKind::Interpolate(items) | ExprKind::Array(items) => {
            for item in items {
                collect_global_expression(item, references);
            }
        }
        ExprKind::Object(record, fields) => {
            if let Some(record) = record {
                references.insert(record.clone());
            }
            for (_, value) in fields {
                collect_global_expression(value, references);
            }
        }
        ExprKind::Unary(_, value) | ExprKind::Member(value, _) => {
            collect_global_expression(value, references)
        }
        ExprKind::Binary(_, left, right) | ExprKind::Index(left, right) => {
            collect_global_expression(left, references);
            collect_global_expression(right, references);
        }
        ExprKind::Call(callee, args) => {
            collect_global_expression(callee, references);
            for arg in args {
                collect_global_expression(arg, references);
            }
        }
        ExprKind::Null
        | ExprKind::Bool(_)
        | ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::String(_) => {}
    }
}

fn collect_type(ty: &Type, references: &mut BTreeSet<String>) {
    match ty {
        Type::Named(name) => {
            references.insert(name.clone());
        }
        Type::Array(inner) | Type::Task(inner) => collect_type(inner, references),
    }
}
