use super::{
    lexer::{lex, Kind, Token},
    syntax::*,
};
use std::collections::BTreeMap;

pub fn parse(file: &str, source: &str) -> Result<Program> {
    let tokens = lex(file, source).map_err(|mut error| {
        error.phase = DiagnosticPhase::Syntax;
        error
    })?;
    Parser {
        tokens,
        i: 0,
        depth: 0,
    }
    .program().map_err(|mut error| {
        error.phase = DiagnosticPhase::Syntax;
        error
    })
}
struct Parser {
    tokens: Vec<Token>,
    i: usize,
    depth: usize,
}
impl Parser {
    fn site(&self) -> Site {
        self.tokens[self.i].site.clone()
    }
    fn is(&self, text: &str) -> bool {
        matches!(&self.tokens[self.i].kind,Kind::Word(s)|Kind::Symbol(s) if s==text)
    }
    fn eat(&mut self, text: &str) -> bool {
        if self.is(text) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn need(&mut self, text: &str) -> Result<()> {
        if self.eat(text) {
            Ok(())
        } else {
            Err(Error::at(&self.site(), format!("expected '{text}'")))
        }
    }
    fn word(&mut self) -> Result<String> {
        if let Kind::Word(s) = &self.tokens[self.i].kind {
            let s = s.clone();
            self.i += 1;
            Ok(s)
        } else {
            Err(Error::at(&self.site(), "expected identifier"))
        }
    }
    fn ty(&mut self) -> Result<Type> {
        self.depth += 1;
        if self.depth > 128 {
            return Err(Error::at(&self.site(), "parser nesting exceeds 128"));
        }
        let result = self.type_inner();
        self.depth -= 1;
        result
    }
    fn type_inner(&mut self) -> Result<Type> {
        let n = self.word()?;
        let mut t = if n == "Task" {
            self.need("<")?;
            let t = self.ty()?;
            self.need(">")?;
            Type::Task(Box::new(t))
        } else {
            Type::Named(n)
        };
        let mut rank = 0;
        while self.eat("[") {
            rank += 1;
            if rank + self.depth > 128 {
                return Err(Error::at(&self.site(), "type nesting exceeds 128"));
            }
            self.need("]")?;
            t = Type::Array(Box::new(t));
        }
        Ok(t)
    }
    fn params(&mut self) -> Result<Vec<Param>> {
        self.need("(")?;
        let mut out = Vec::new();
        if !self.eat(")") {
            loop {
                let ty = self.ty()?;
                let name = self.word()?;
                let site = self.tokens[self.i - 1].site.clone();
                let default = if self.eat("=") {
                    Some(self.expr(0)?)
                } else {
                    None
                };
                out.push(Param { name, ty, site, default });
                if self.eat(")") {
                    break;
                }
                self.need(",")?;
            }
        }
        Ok(out)
    }
    fn config(&mut self) -> Result<BTreeMap<String, Expr>> {
        self.need("{")?;
        let mut map = BTreeMap::new();
        while !self.eat("}") {
            let site = self.site();
            let key = self.word()?;
            self.need("=")?;
            let v = self.expr(0)?;
            self.need(";")?;
            if map.insert(key.clone(), v).is_some() {
                return Err(Error::at(&site, format!("duplicate property '{key}'")));
            }
        }
        Ok(map)
    }
    fn program(mut self) -> Result<Program> {
        let mut p = Program::default();
        while self.tokens[self.i].kind != Kind::End {
            let site = self.site();
            if self.eat("version") {
                if p.language_version.is_some() {
                    return Err(Error::at(&site, "duplicate language version declaration"));
                }
                let version = match self.tokens[self.i].kind.clone() {
                    Kind::Text(version, false) => {
                        self.i += 1;
                        version
                    }
                    _ => return Err(Error::at(&self.site(), "version requires a quoted string")),
                };
                self.need(";")?;
                p.language_version = Some(version);
                continue;
            }
            if self.eat("using") || self.eat("import") {
                if let Kind::Text(path, false) = self.tokens[self.i].kind.clone() {
                    self.i += 1;
                    self.need(";")?;
                    p.imports.push((path, site));
                    continue;
                }
                return Err(Error::at(
                    &site,
                    "imports require a quoted relative .camel path",
                ));
            }
            let flavor = if self.eat("record") {
                "record"
            } else if self.eat("agent") {
                "agent"
            } else if self.eat("policy") {
                "policy"
            } else if self.eat("tool") {
                "tool"
            } else if self.eat("workflow") {
                "workflow"
            } else {
                "function"
            };
            let (name, ret) = if ["record", "agent", "policy"].contains(&flavor)
                || (flavor == "workflow"
                    && matches!(self.tokens.get(self.i+1).map(|t|&t.kind),Some(Kind::Symbol(s)) if s=="("))
            {
                (self.word()?, Type::named("void"))
            } else {
                let t = self.ty()?;
                (self.word()?, t)
            };
            let binding_site = self.tokens[self.i - 1].site.clone();
            if p.sites.insert(name.clone(), binding_site).is_some() {
                return Err(Error::at(&site, format!("duplicate declaration '{name}'")));
            }
            match flavor {
                "record" => {
                    self.need("{")?;
                    let mut fields = BTreeMap::new();
                    while !self.eat("}") {
                        let ty = self.ty()?;
                        let field = self.word()?;
                        self.need(";")?;
                        if fields.insert(field.clone(), ty).is_some() {
                            return Err(Error::at(&site, format!("duplicate field '{field}'")));
                        }
                    }
                    p.records.insert(name, fields);
                }
                "agent" => {
                    p.agents.insert(name, self.config()?);
                }
                "policy" => {
                    p.policies.insert(name, self.config()?);
                }
                "tool" => {
                    let params = self.params()?;
                    let config = self.config()?;
                    p.tools.insert(
                        name,
                        Tool {
                            params,
                            returns: ret,
                            config,
                            site,
                        },
                    );
                }
                _ => {
                    let params = self.params()?;
                    let body = self.block()?;
                    p.functions.insert(
                        name.clone(),
                        Function {
                            params,
                            returns: ret,
                            body,
                            workflow: flavor == "workflow",
                            site,
                        },
                    );
                }
            }
        }
        Ok(p)
    }
    fn block(&mut self) -> Result<Stmt> {
        let site = self.site();
        self.need("{")?;
        let mut b = Vec::new();
        while !self.eat("}") {
            if self.tokens[self.i].kind == Kind::End {
                return Err(Error::at(&site, "unterminated block"));
            }
            b.push(self.stmt()?);
        }
        Ok(Stmt {
            kind: StmtKind::Block(b),
            site,
        })
    }
    fn condition(&mut self) -> Result<Expr> {
        self.need("(")?;
        let e = self.expr(0)?;
        self.need(")")?;
        Ok(e)
    }
    fn stmt(&mut self) -> Result<Stmt> {
        self.depth += 1;
        if self.depth > 128 {
            return Err(Error::at(&self.site(), "parser nesting exceeds 128"));
        }
        let result = self.statement_inner();
        self.depth -= 1;
        result
    }
    fn statement_inner(&mut self) -> Result<Stmt> {
        let mut site = self.site();
        let kind = if self.is("{") {
            return self.block();
        } else if self.eat("if") {
            let c = self.condition()?;
            let b = Box::new(self.stmt()?);
            let other = if self.eat("else") {
                Some(Box::new(self.stmt()?))
            } else {
                None
            };
            StmtKind::If(c, b, other)
        } else if self.eat("while") {
            let c = self.condition()?;
            StmtKind::While(c, Box::new(self.stmt()?))
        } else if self.eat("for") {
            self.need("(")?;
            let init = Box::new(self.simple(true)?);
            let c = self.expr(0)?;
            self.need(";")?;
            let step = Box::new(self.simple(false)?);
            self.need(")")?;
            StmtKind::For(init, c, step, Box::new(self.stmt()?))
        } else if self.eat("foreach") {
            self.need("(")?;
            let ty = if self.eat("var") {
                None
            } else {
                Some(self.ty()?)
            };
            let name = self.word()?;
            site = self.tokens[self.i - 1].site.clone();
            self.need("in")?;
            let e = self.expr(0)?;
            self.need(")")?;
            StmtKind::Each(name, ty, e, Box::new(self.stmt()?))
        } else if self.eat("return") {
            let e = if self.is(";") {
                None
            } else {
                Some(self.expr(0)?)
            };
            self.need(";")?;
            StmtKind::Return(e)
        } else if self.eat("break") {
            self.need(";")?;
            StmtKind::Break
        } else if self.eat("continue") {
            self.need(";")?;
            StmtKind::Continue
        } else if self.eat("throw") {
            let e = self.expr(0)?;
            self.need(";")?;
            StmtKind::Throw(e)
        } else if self.eat("try") {
            let b = Box::new(self.block()?);
            let catch = if self.eat("catch") {
                self.need("(")?;
                let n = self.word()?;
                self.need(")")?;
                Some((n, Box::new(self.block()?)))
            } else {
                None
            };
            let finally = if self.eat("finally") {
                Some(Box::new(self.block()?))
            } else {
                None
            };
            if catch.is_none() && finally.is_none() {
                return Err(Error::at(&site, "try requires catch or finally"));
            }
            StmtKind::Try(b, catch, finally)
        } else {
            return self.simple(true);
        };
        Ok(Stmt { kind, site })
    }
    fn simple(&mut self, semicolon: bool) -> Result<Stmt> {
        let mut site = self.site();
        let saved = self.i;
        let declaration = if self.eat("var") {
            Some(None)
        } else {
            match self.ty() {
                Ok(t)
                    if matches!(self.tokens[self.i].kind, Kind::Word(_))
                        && matches!(self.tokens.get(self.i+1).map(|t|&t.kind),Some(Kind::Symbol(s)) if s=="=") =>
                {
                    Some(Some(t))
                }
                _ => {
                    self.i = saved;
                    None
                }
            }
        };
        let kind = if let Some(ty) = declaration {
            let name = self.word()?;
            site = self.tokens[self.i - 1].site.clone();
            self.need("=")?;
            StmtKind::Let(name, ty, self.expr(0)?)
        } else {
            let left = self.expr(0)?;
            if self.eat("=") {
                StmtKind::Assign(left, self.expr(0)?)
            } else if self.is("+=") || self.is("-=") || self.is("++") || self.is("--") {
                let op = match &self.tokens[self.i].kind {
                    Kind::Symbol(s) => s.clone(),
                    _ => unreachable!(),
                };
                self.i += 1;
                let rhs = if op.len() == 2 && op.chars().nth(1) == op.chars().next() {
                    Expr {
                        kind: ExprKind::Int(1),
                        site: site.clone(),
                    }
                } else {
                    self.expr(0)?
                };
                StmtKind::Update(left, op[..1].into(), rhs)
            } else {
                StmtKind::Expr(left)
            }
        };
        if semicolon {
            self.need(";")?;
        }
        Ok(Stmt { kind, site })
    }
    fn expr(&mut self, min: u8) -> Result<Expr> {
        self.depth += 1;
        if self.depth > 128 {
            return Err(Error::at(&self.site(), "parser nesting exceeds 128"));
        }
        let result = self.expression_inner(min);
        self.depth -= 1;
        if let Ok(e) = &result {
            expression_depth(e, 0)?;
        }
        result
    }
    fn expression_inner(&mut self, min: u8) -> Result<Expr> {
        let site = self.site();
        let token = self.tokens[self.i].kind.clone();
        self.i += 1;
        let kind = match token {
            Kind::Number(s) => {
                if s.contains('.') {
                    ExprKind::Float(s.parse().map_err(|_| Error::at(&site, "invalid float"))?)
                } else {
                    ExprKind::Int(
                        s.parse()
                            .map_err(|_| Error::at(&site, "integer out of range"))?,
                    )
                }
            }
            Kind::Text(s, false) => ExprKind::String(s),
            Kind::Text(s, true) => ExprKind::Interpolate(self.interpolation(&s, &site)?),
            Kind::Word(n) if n == "true" || n == "false" => ExprKind::Bool(n == "true"),
            Kind::Word(n) if n == "null" => ExprKind::Null,
            Kind::Word(n) if n == "await" || n == "spawn" => {
                ExprKind::Unary(n, Box::new(self.expr(7)?))
            }
            Kind::Word(n) if n == "new" => {
                let ty = if self.is("{") {
                    None
                } else {
                    Some(self.word()?)
                };
                self.need("{")?;
                let mut fields = Vec::<(String, Expr)>::new();
                if !self.eat("}") {
                    loop {
                        let key = self.word()?;
                        self.need("=")?;
                        let val = self.expr(0)?;
                        if fields.iter().any(|(k, _)| k == &key) {
                            return Err(Error::at(&site, format!("duplicate field '{key}'")));
                        }
                        fields.push((key, val));
                        if self.eat("}") {
                            break;
                        }
                        self.need(",")?;
                    }
                }
                ExprKind::Object(ty, fields)
            }
            Kind::Word(n) => ExprKind::Name(n),
            Kind::Symbol(s) if s == "!" || s == "-" || s == "+" => {
                ExprKind::Unary(s, Box::new(self.expr(7)?))
            }
            Kind::Symbol(s) if s == "(" => {
                let e = self.expr(0)?;
                self.need(")")?;
                e.kind
            }
            Kind::Symbol(s) if s == "[" => {
                let mut values = Vec::new();
                if !self.eat("]") {
                    loop {
                        values.push(self.expr(0)?);
                        if self.eat("]") {
                            break;
                        }
                        self.need(",")?;
                    }
                }
                ExprKind::Array(values)
            }
            _ => return Err(Error::at(&site, "expected expression")),
        };
        let mut left = Expr {
            kind,
            site: site.clone(),
        };
        let mut chain = 0;
        loop {
            chain += 1;
            if chain > 128 {
                return Err(Error::at(&site, "expression chain exceeds 128"));
            }
            if self.eat(".") {
                let n = self.word()?;
                left = Expr {
                    kind: ExprKind::Member(Box::new(left), n),
                    site: site.clone(),
                };
                continue;
            }
            if self.eat("[") {
                let idx = self.expr(0)?;
                self.need("]")?;
                left = Expr {
                    kind: ExprKind::Index(Box::new(left), Box::new(idx)),
                    site: site.clone(),
                };
                continue;
            }
            if self.eat("(") {
                let mut args = Vec::new();
                if !self.eat(")") {
                    loop {
                        args.push(self.expr(0)?);
                        if self.eat(")") {
                            break;
                        }
                        self.need(",")?;
                    }
                }
                left = Expr {
                    kind: ExprKind::Call(Box::new(left), args),
                    site: site.clone(),
                };
                continue;
            }
            let op = match &self.tokens[self.i].kind {
                Kind::Symbol(s) => s.clone(),
                _ => break,
            };
            let prec = match op.as_str() {
                "||" => 1,
                "&&" => 2,
                "==" | "!=" => 3,
                "<" | "<=" | ">" | ">=" => 4,
                "+" | "-" => 5,
                "*" | "/" | "%" => 6,
                _ => break,
            };
            if prec < min {
                break;
            }
            self.i += 1;
            let right = self.expr(prec + 1)?;
            left = Expr {
                kind: ExprKind::Binary(op, Box::new(left), Box::new(right)),
                site: site.clone(),
            };
        }
        Ok(left)
    }
    fn interpolation(&self, s: &str, site: &Site) -> Result<Vec<Expr>> {
        let mut out = Vec::new();
        let mut literal = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '{' {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    literal.push('{');
                    continue;
                }
                if !literal.is_empty() {
                    out.push(Expr {
                        kind: ExprKind::String(std::mem::take(&mut literal)),
                        site: site.clone(),
                    });
                }
                let mut inner = String::new();
                let mut depth = 1;
                let mut quoted = false;
                let mut escaped = false;
                for c in chars.by_ref() {
                    if quoted && escaped {
                        escaped = false;
                        inner.push(c);
                        continue;
                    }
                    if quoted && c == '\\' {
                        escaped = true;
                        inner.push(c);
                        continue;
                    }
                    if c == '"' {
                        quoted = !quoted;
                    }
                    if !quoted && c == '{' {
                        depth += 1;
                    }
                    if !quoted && c == '}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    inner.push(c);
                }
                if depth != 0 {
                    return Err(Error::at(site, "unclosed interpolation"));
                }
                let mut p = Parser {
                    tokens: lex(&site.file, &inner)?,
                    i: 0,
                    depth: self.depth,
                };
                for token in &mut p.tokens {
                    token.site.line += site.line - 1;
                    if token.site.line == site.line {
                        token.site.column += site.column;
                    }
                }
                let e = p.expr(0)?;
                if p.tokens[p.i].kind != Kind::End {
                    return Err(Error::at(site, "invalid interpolation"));
                }
                out.push(e);
            } else if c == '}' {
                if chars.next() != Some('}') {
                    return Err(Error::at(site, "escape literal '}' as '}}'"));
                }
                literal.push('}');
            } else {
                literal.push(c);
            }
        }
        out.push(Expr {
            kind: ExprKind::String(literal),
            site: site.clone(),
        });
        Ok(out)
    }
}

fn expression_depth(e: &Expr, depth: usize) -> Result<()> {
    if depth > 128 {
        return Err(Error::at(&e.site, "expression nesting exceeds 128"));
    }
    match &e.kind {
        ExprKind::Array(items) | ExprKind::Interpolate(items) => {
            for e in items {
                expression_depth(e, depth + 1)?;
            }
        }
        ExprKind::Object(_, fields) => {
            for (_, e) in fields {
                expression_depth(e, depth + 1)?;
            }
        }
        ExprKind::Unary(_, e) | ExprKind::Member(e, _) => expression_depth(e, depth + 1)?,
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => {
            expression_depth(a, depth + 1)?;
            expression_depth(b, depth + 1)?;
        }
        ExprKind::Call(f, args) => {
            expression_depth(f, depth + 1)?;
            for e in args {
                expression_depth(e, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}
