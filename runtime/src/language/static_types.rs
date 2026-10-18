use super::syntax::*;
use std::collections::BTreeMap;

type Scope = BTreeMap<String, Type>;

pub(super) fn check_function_diagnostics(program: &Program, function: &Function) -> Vec<Error> {
    let checker = StaticChecker { program };
    let mut diagnostics = Vec::new();
    for parameter in &function.params {
        if let Some(default) = &parameter.default {
            if let Err(error) = checker
                .expression(default, &[])
                .and_then(|actual| checker.compatible(&parameter.ty, &actual, &default.site))
            {
                diagnostics.push(error);
            }
        }
    }
    let mut scopes = vec![function
        .params
        .iter()
        .map(|param| (param.name.clone(), param.ty.clone()))
        .collect()];
    if let StmtKind::Block(statements) = &function.body.kind {
        scopes.push(Scope::new());
        for statement in statements {
            if let Err(error) = checker.statement(statement, &mut scopes, &function.returns) {
                diagnostics.push(error);
            }
        }
        scopes.pop();
    } else if let Err(error) = checker.statement(&function.body, &mut scopes, &function.returns) {
        diagnostics.push(error);
    }
    diagnostics
}

pub(super) fn check_tool(program: &Program, tool: &Tool) -> Result<()> {
    let checker = StaticChecker { program };
    for parameter in &tool.params {
        if let Some(default) = &parameter.default {
            let actual = checker.expression(default, &[])?;
            checker.compatible(&parameter.ty, &actual, &default.site)?;
        }
    }
    Ok(())
}

struct StaticChecker<'a> {
    program: &'a Program,
}

impl StaticChecker<'_> {
    fn statement(&self, statement: &Stmt, scopes: &mut Vec<Scope>, returns: &Type) -> Result<()> {
        match &statement.kind {
            StmtKind::Block(statements) => {
                scopes.push(Scope::new());
                for statement in statements {
                    self.statement(statement, scopes, returns)?;
                }
                scopes.pop();
            }
            StmtKind::Let(name, declared, value) => {
                let actual = self.expression(value, scopes)?;
                let ty = declared.clone().unwrap_or_else(|| actual.clone());
                if let Some(declared) = declared {
                    self.compatible(declared, &actual, &value.site)?;
                }
                scopes.last_mut().unwrap().insert(name.clone(), ty);
            }
            StmtKind::Assign(target, value) => {
                let expected = self.expression_type(target, scopes)?;
                let actual = self.expression(value, scopes)?;
                self.compatible(&expected, &actual, &value.site)?;
            }
            StmtKind::Update(target, operator, value) => {
                let left = self.expression_type(target, scopes)?;
                let right = self.expression(value, scopes)?;
                if operator == "=" {
                    self.compatible(&left, &right, &value.site)?;
                } else {
                    self.numeric_pair(&left, &right, &statement.site)?;
                }
            }
            StmtKind::Expr(expression) | StmtKind::Throw(expression) => {
                self.expression(expression, scopes)?;
            }
            StmtKind::Return(value) => match value {
                Some(value) => {
                    let actual = self.expression(value, scopes)?;
                    self.compatible(returns, &actual, &value.site)?;
                }
                None if *returns != Type::named("void") => {
                    return Err(Error::at(
                        &statement.site,
                        "non-void function requires a return value",
                    ));
                }
                None => {}
            },
            StmtKind::If(condition, yes, no) => {
                self.require_bool(condition, scopes)?;
                self.branch(yes, scopes, returns)?;
                if let Some(no) = no {
                    self.branch(no, scopes, returns)?;
                }
            }
            StmtKind::While(condition, body) => {
                self.require_bool(condition, scopes)?;
                self.branch(body, scopes, returns)?;
            }
            StmtKind::For(initializer, condition, step, body) => {
                scopes.push(Scope::new());
                self.statement(initializer, scopes, returns)?;
                self.require_bool(condition, scopes)?;
                self.statement(step, scopes, returns)?;
                self.branch(body, scopes, returns)?;
                scopes.pop();
            }
            StmtKind::Each(name, declared, iterable, body) => {
                let iterable_type = self.expression(iterable, scopes)?;
                let item_type = match iterable_type {
                    Type::Array(item) => *item,
                    Type::Named(name) if name == "string" => Type::named("string"),
                    Type::Named(name) if name == "any" || name == "object" => Type::named("any"),
                    other => {
                        return Err(Error::at(
                            &iterable.site,
                            format!("foreach requires an array or string, got {other}"),
                        ))
                    }
                };
                let binding_type = declared.clone().unwrap_or(item_type.clone());
                self.compatible(&binding_type, &item_type, &statement.site)?;
                scopes.push([(name.clone(), binding_type)].into_iter().collect());
                self.branch(body, scopes, returns)?;
                scopes.pop();
            }
            StmtKind::Try(body, catch, finally) => {
                self.branch(body, scopes, returns)?;
                if let Some((name, body)) = catch {
                    scopes.push(
                        [(name.clone(), Type::named("object"))]
                            .into_iter()
                            .collect(),
                    );
                    self.branch(body, scopes, returns)?;
                    scopes.pop();
                }
                if let Some(body) = finally {
                    self.branch(body, scopes, returns)?;
                }
            }
            StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Checkpoint(_) => {}
        }
        Ok(())
    }

    fn branch(&self, statement: &Stmt, scopes: &mut Vec<Scope>, returns: &Type) -> Result<()> {
        scopes.push(Scope::new());
        let result = self.statement(statement, scopes, returns);
        scopes.pop();
        result
    }

    fn require_bool(&self, expression: &Expr, scopes: &[Scope]) -> Result<()> {
        let actual = self.expression(expression, scopes)?;
        self.compatible(&Type::named("bool"), &actual, &expression.site)
    }

    fn expression_type(&self, expression: &Expr, scopes: &[Scope]) -> Result<Type> {
        match &expression.kind {
            ExprKind::Name(name) => {
                if let Some(ty) = scopes.iter().rev().find_map(|scope| scope.get(name)) {
                    return Ok(ty.clone());
                }
                Ok(if self.program.functions.contains_key(name) {
                    Type::named("Function")
                } else if self.program.tools.contains_key(name) {
                    Type::named("Tool")
                } else if self.program.agents.contains_key(name) {
                    Type::named("Agent")
                } else if self.program.records.contains_key(name)
                    || ["json", "audio", "clock"].contains(&name.as_str())
                {
                    Type::named("Function")
                } else {
                    Type::named("any")
                })
            }
            ExprKind::Member(base, name) => {
                let base_type = self.expression(base, scopes)?;
                match (&base_type, name.as_str()) {
                    (Type::Named(n), "length") if n == "string" => Ok(Type::named("int")),
                    (Type::Array(_), "length") => Ok(Type::named("int")),
                    (Type::Named(n), field) if n == "AgentResult" => Ok(match field {
                        "text" | "model" => Type::named("string"),
                        "toolCalls" => Type::named("int"),
                        "data" => Type::named("any"),
                        "provenance" => Type::named("EvidenceProvenance"),
                        _ => Type::named("any"),
                    }),
                    (Type::Named(n), field) if n == "EvidenceProvenance" => Ok(match field {
                        "agent" | "model" | "file" | "sourceFingerprint" => Type::named("string"),
                        "line" | "column" | "offset" | "endLine" | "endColumn" | "endOffset" => {
                            Type::named("int")
                        }
                        "tools" => Type::Array(Box::new(Type::named("string"))),
                        _ => Type::named("any"),
                    }),
                    (Type::Named(n), field) if self.program.records.contains_key(n) => self
                        .program
                        .records
                        .get(n)
                        .and_then(|fields| fields.get(field))
                        .cloned()
                        .ok_or_else(|| {
                            Error::at(
                                &expression.site,
                                format!("record '{n}' has no field '{field}'"),
                            )
                        }),
                    (Type::Named(n), "run") if n == "Agent" => Ok(Type::named("Function")),
                    (Type::Named(n), _) if n == "any" || n == "object" => Ok(Type::named("any")),
                    (Type::Named(n), _) if ["json", "audio", "clock"].contains(&n.as_str()) => {
                        Ok(Type::named("Function"))
                    }
                    _ => Ok(Type::named("any")),
                }
            }
            ExprKind::Index(base, index) => {
                let index_type = self.expression(index, scopes)?;
                self.compatible(&Type::named("int"), &index_type, &index.site)?;
                match self.expression(base, scopes)? {
                    Type::Array(item) => Ok(*item),
                    Type::Named(n) if n == "string" => Ok(Type::named("string")),
                    _ => Ok(Type::named("any")),
                }
            }
            _ => self.expression(expression, scopes),
        }
    }

    fn expression(&self, expression: &Expr, scopes: &[Scope]) -> Result<Type> {
        let ty = match &expression.kind {
            ExprKind::Null => Type::named("any"),
            ExprKind::Bool(_) => Type::named("bool"),
            ExprKind::Int(_) => Type::named("int"),
            ExprKind::Float(_) => Type::named("double"),
            ExprKind::String(_) | ExprKind::Interpolate(_) => {
                if let ExprKind::Interpolate(parts) = &expression.kind {
                    for part in parts {
                        self.expression(part, scopes)?;
                    }
                }
                Type::named("string")
            }
            ExprKind::Name(_) | ExprKind::Member(_, _) | ExprKind::Index(_, _) => {
                self.expression_type(expression, scopes)?
            }
            ExprKind::Array(items) => {
                let mut item_type = None;
                for item in items {
                    let actual = self.expression(item, scopes)?;
                    item_type = Some(match item_type {
                        None => actual,
                        Some(previous) if previous == actual => previous,
                        _ => Type::named("any"),
                    });
                }
                Type::Array(Box::new(item_type.unwrap_or_else(|| Type::named("any"))))
            }
            ExprKind::Object(record, fields) => {
                for (_, value) in fields {
                    self.expression(value, scopes)?;
                }
                record
                    .as_ref()
                    .map(|name| Type::named(name))
                    .unwrap_or_else(|| Type::named("object"))
            }
            ExprKind::Unary(operator, value) => {
                let actual = self.expression(value, scopes)?;
                match operator.as_str() {
                    "!" => {
                        self.compatible(&Type::named("bool"), &actual, &value.site)?;
                        Type::named("bool")
                    }
                    "+" | "-" => {
                        self.numeric_pair(&actual, &actual, &value.site)?;
                        actual
                    }
                    "spawn" => Type::Task(Box::new(actual)),
                    "await" => match actual {
                        Type::Task(value) => *value,
                        Type::Named(n) if n == "any" => Type::named("any"),
                        other => other,
                    },
                    _ => Type::named("any"),
                }
            }
            ExprKind::Binary(operator, left, right) => {
                let left_type = self.expression(left, scopes)?;
                let right_type = self.expression(right, scopes)?;
                match operator.as_str() {
                    "&&" | "||" => {
                        self.compatible(&Type::named("bool"), &left_type, &left.site)?;
                        self.compatible(&Type::named("bool"), &right_type, &right.site)?;
                        Type::named("bool")
                    }
                    "==" | "!=" => Type::named("bool"),
                    "<" | "<=" | ">" | ">=" => {
                        let strings = left_type == Type::named("string")
                            && right_type == Type::named("string");
                        if !strings {
                            self.numeric_pair(&left_type, &right_type, &expression.site)?;
                        }
                        Type::named("bool")
                    }
                    "+" if left_type == Type::named("string")
                        && right_type == Type::named("string") =>
                    {
                        Type::named("string")
                    }
                    "+" | "-" | "*" | "/" | "%" => {
                        self.numeric_pair(&left_type, &right_type, &expression.site)?;
                        if left_type == Type::named("double") || right_type == Type::named("double")
                        {
                            Type::named("double")
                        } else {
                            Type::named("int")
                        }
                    }
                    _ => Type::named("any"),
                }
            }
            ExprKind::Call(callee, arguments) => {
                let argument_types = arguments
                    .iter()
                    .map(|argument| self.expression(argument, scopes))
                    .collect::<Result<Vec<_>>>()?;
                if let ExprKind::Name(name) = &callee.kind {
                    if let Some(local_type) = scopes.iter().rev().find_map(|scope| scope.get(name))
                    {
                        if local_type != &Type::named("Function")
                            && local_type != &Type::named("any")
                        {
                            return Err(Error::at(
                                &callee.site,
                                format!("value of type {local_type} is not callable"),
                            ));
                        }
                        Type::named("any")
                    } else if let Some(function) = self.program.functions.get(name) {
                        self.call_arguments(&function.params, arguments, &argument_types)?;
                        function.returns.clone()
                    } else if let Some(tool) = self.program.tools.get(name) {
                        self.call_arguments(&tool.params, arguments, &argument_types)?;
                        tool.returns.clone()
                    } else {
                        self.builtin_call(name, &expression.site)?
                    }
                } else if let ExprKind::Member(base, name) = &callee.kind {
                    let base_type = self.expression(base, scopes)?;
                    if name == "run" && base_type == Type::named("Agent") {
                        Type::named("AgentResult")
                    } else if base_type == Type::named("Function")
                        || base_type == Type::named("any")
                        || base_type == Type::named("Tool")
                    {
                        Type::named("any")
                    } else {
                        self.expression(callee, scopes)?;
                        Type::named("any")
                    }
                } else {
                    let callee_type = self.expression(callee, scopes)?;
                    if callee_type != Type::named("Function") && callee_type != Type::named("any") {
                        return Err(Error::at(
                            &callee.site,
                            format!("value of type {callee_type} is not callable"),
                        ));
                    }
                    Type::named("any")
                }
            }
        };
        Ok(ty)
    }

    fn builtin_call(&self, name: &str, site: &Site) -> Result<Type> {
        Ok(match name {
            "log" => Type::named("void"),
            "env" | "string" => Type::named("string"),
            "int" | "len" => Type::named("int"),
            "double" => Type::named("double"),
            "assert" => Type::named("void"),
            "range" => Type::Array(Box::new(Type::named("int"))),
            "record" | "audio.load" => Type::named("Audio"),
            "clock.now" => Type::named("int"),
            "json.stringify" => Type::named("string"),
            "json.parse" => Type::named("any"),
            other => {
                return Err(Error::at(
                    site,
                    format!("cannot statically type built-in '{other}'"),
                ))
            }
        })
    }

    fn call_arguments(&self, params: &[Param], args: &[Expr], actual: &[Type]) -> Result<()> {
        for ((parameter, argument), actual) in params.iter().zip(args).zip(actual) {
            self.compatible(&parameter.ty, actual, &argument.site)?;
        }
        Ok(())
    }

    fn numeric_pair(&self, left: &Type, right: &Type, site: &Site) -> Result<()> {
        for ty in [left, right] {
            if !matches!(ty, Type::Named(n) if ["int", "double", "any"].contains(&n.as_str())) {
                return Err(Error::at(
                    site,
                    format!("numeric operation requires int or double, got {ty}"),
                ));
            }
        }
        Ok(())
    }

    fn compatible(&self, expected: &Type, actual: &Type, site: &Site) -> Result<()> {
        let actual_is_object = *actual == Type::named("object")
            || matches!(actual, Type::Named(name) if self.program.records.contains_key(name));
        let compatible = expected == actual
            || *expected == Type::named("any")
            || *actual == Type::named("any")
            || (*expected == Type::named("object") && actual_is_object)
            || (*expected == Type::named("double") && *actual == Type::named("int"))
            || matches!((expected, actual), (Type::Array(a), Type::Array(b)) if self.compatible(a, b, site).is_ok())
            || matches!((expected, actual), (Type::Task(a), Type::Task(b)) if self.compatible(a, b, site).is_ok());
        if compatible {
            Ok(())
        } else {
            Err(Error::at(
                site,
                format!("type mismatch: expected {expected}, got {actual}"),
            ))
        }
    }
}
