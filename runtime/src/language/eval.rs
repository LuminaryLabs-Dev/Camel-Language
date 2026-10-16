use super::{
    check::{arity, BUILTINS},
    host::{count, emit, ExecutionEvent, Host, Operation},
    syntax::*,
    value::{drain, ensure_type, wait, TaskState, Value},
    CompiledProgram,
};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
    thread,
};

#[derive(Clone, Debug)]
pub struct Limits {
    pub steps: usize,
    pub call_depth: usize,
    pub tasks: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            steps: 100_000,
            call_depth: 128,
            tasks: 64,
        }
    }
}
pub struct Execution {
    pub value: Value,
    pub logs: Vec<String>,
}
pub struct Engine {
    compiled: CompiledProgram,
    host: Host,
    limits: Limits,
}
impl Engine {
    pub fn new(compiled: CompiledProgram, host: Host) -> Self {
        Self {
            compiled,
            host,
            limits: Limits::default(),
        }
    }
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }
    pub fn run(&self, entry: &str, args: Vec<Value>) -> Result<Execution> {
        self.compiled.entry_parameters(entry)?;
        let site = self.compiled.program.functions[entry].site.clone();
        let mut operation = Operation::begin(&self.host, "workflow", entry, "", &site);
        let shared = Arc::new(Shared {
            compiled: self.compiled.clone(),
            host: self.host.clone(),
            limits: self.limits.clone(),
            fuel: AtomicUsize::new(self.limits.steps),
            tasks: Mutex::new(Vec::new()),
            logs: Mutex::new(Vec::new()),
            microphone: Mutex::new(()),
        });
        let mut ctx = Context {
            shared: shared.clone(),
            scopes: vec![BTreeMap::new()],
            depth: 0,
        };
        let mut result = ctx.function(entry, args, &site);
        // Structured lifetime: even un-awaited tasks finish before run() returns.
        let mut index = 0;
        loop {
            let task = shared.tasks.lock().unwrap().get(index).cloned();
            if let Some(task) = task {
                let _ = drain(&task);
                index += 1;
            } else {
                break;
            }
        }
        for task in shared.tasks.lock().unwrap().iter() {
            if !task.2.load(Ordering::Acquire) {
                if let Err(e) = drain(task) {
                    if result.is_ok() {
                        result = Err(e);
                    }
                }
            }
        }
        let logs = shared.logs.lock().unwrap().clone();
        operation.success = result.is_ok();
        Ok(Execution {
            value: result?,
            logs,
        })
    }
}
pub(crate) struct Shared {
    pub compiled: CompiledProgram,
    pub host: Host,
    pub limits: Limits,
    fuel: AtomicUsize,
    tasks: Mutex<Vec<TaskState>>,
    logs: Mutex<Vec<String>>,
    microphone: Mutex<()>,
}
#[derive(Clone)]
struct Binding {
    value: Value,
    ty: Type,
}
#[derive(Clone)]
pub(crate) struct Context {
    pub shared: Arc<Shared>,
    scopes: Vec<BTreeMap<String, Binding>>,
    depth: usize,
}
#[derive(Clone)]
enum Flow {
    Next,
    Return(Value),
    Break,
    Continue,
}
impl Context {
    pub(crate) fn global_context(&self) -> Self {
        Self {
            shared: self.shared.clone(),
            scopes: vec![BTreeMap::new()],
            depth: self.depth,
        }
    }
    pub(crate) fn tick(&self, site: &Site) -> Result<()> {
        let fuel = &self.shared.fuel;
        let mut remaining = fuel.load(Ordering::Relaxed);
        loop {
            let Some(next) = remaining.checked_sub(1) else {
                let mut error = Error::at(site, "execution step budget exhausted");
                error.fatal = true;
                return Err(error);
            };
            match fuel.compare_exchange_weak(remaining, next, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return Ok(()),
                Err(observed) => remaining = observed,
            }
        }
    }
    fn name(&self, name: &str, s: &Site) -> Result<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(b) = scope.get(name) {
                return Ok(b.value.clone());
            }
        }
        let p = &self.shared.compiled.program;
        if p.functions.contains_key(name) {
            Ok(Value::Function(name.into()))
        } else if p.agents.contains_key(name) {
            Ok(Value::Agent(name.into()))
        } else if p.tools.contains_key(name) {
            Ok(Value::Tool(name.into()))
        } else if p.records.contains_key(name) || p.policies.contains_key(name) {
            Ok(Value::Reference(name.into()))
        } else if BUILTINS.contains(&name) {
            Ok(Value::Builtin(name.into()))
        } else {
            Err(Error::at(s, format!("unknown name '{name}'")))
        }
    }
    fn bind(&mut self, name: &str, ty: Option<&Type>, value: Value, s: &Site) -> Result<()> {
        let ty = ty.cloned().unwrap_or_else(|| inferred(&value));
        ensure_type(&self.shared.compiled.program, &ty, &value, s)?;
        let scope = self.scopes.last_mut().unwrap();
        if scope.contains_key(name) {
            return Err(Error::at(s, format!("duplicate variable '{name}'")));
        }
        let value = typed_task(value, &ty);
        scope.insert(name.into(), Binding { value, ty });
        Ok(())
    }
    pub(crate) fn eval(&mut self, e: &Expr) -> Result<Value> {
        self.tick(&e.site)?;
        let s = &e.site;
        match &e.kind {
            ExprKind::Null => Ok(Value::Null),
            ExprKind::Bool(v) => Ok(Value::Bool(*v)),
            ExprKind::Int(v) => Ok(Value::Int(*v)),
            ExprKind::Float(v) => Ok(Value::Float(*v)),
            ExprKind::String(v) => Ok(Value::String(v.clone())),
            ExprKind::Name(n) => self.name(n, s),
            ExprKind::Array(items) => Ok(Value::Array(
                items.iter().map(|e| self.eval(e)).collect::<Result<_>>()?,
            )),
            ExprKind::Interpolate(items) => {
                let mut text = String::new();
                for e in items {
                    text.push_str(&self.eval(e)?.display());
                }
                Ok(Value::String(text))
            }
            ExprKind::Object(ty, fields) => {
                let value = Value::Object(
                    fields
                        .iter()
                        .map(|(k, e)| Ok((k.clone(), self.eval(e)?)))
                        .collect::<Result<_>>()?,
                );
                if let Some(n) = ty {
                    ensure_type(&self.shared.compiled.program, &Type::named(n), &value, s)?;
                }
                Ok(value)
            }
            ExprKind::Unary(op, e) if op == "spawn" => {
                let task: TaskState = Arc::new((
                    Mutex::new(None),
                    Condvar::new(),
                    std::sync::atomic::AtomicBool::new(false),
                ));
                {
                    let mut tasks = self.shared.tasks.lock().unwrap();
                    if tasks.len() >= self.shared.limits.tasks {
                        let mut error = Error::at(s, "task budget exhausted");
                        error.fatal = true;
                        return Err(error);
                    }
                    tasks.push(task.clone());
                }
                let state = task.clone();
                let mut child = self.clone();
                let e = (**e).clone();
                if let Err(error) =
                    thread::Builder::new()
                        .name("camel-task".into())
                        .spawn(move || {
                            let result =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    child.eval(&e)
                                }))
                                .unwrap_or_else(|_| {
                                    let mut error = Error::at(&e.site, "host task panicked");
                                    error.fatal = true;
                                    Err(error)
                                });
                            *state.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
                            state.1.notify_all();
                        })
                {
                    let error = Error::at(s, error.to_string());
                    *task.0.lock().unwrap() = Some(Err(error.clone()));
                    task.1.notify_all();
                    return Err(error);
                }
                Ok(Value::Task(task, None))
            }
            ExprKind::Unary(op, e) => {
                let v = self.eval(e)?;
                match (op.as_str(), v) {
                    ("await", Value::Task(task, ty)) => {
                        let result = wait(&task)?;
                        if let Some(ty) = ty {
                            ensure_type(&self.shared.compiled.program, &ty, &result, s)?;
                        }
                        Ok(result)
                    }
                    ("await", v) => Ok(v),
                    ("!", v) => Ok(Value::Bool(!v.boolean(s)?)),
                    ("-", Value::Int(v)) => v
                        .checked_neg()
                        .map(Value::Int)
                        .ok_or_else(|| Error::at(s, "integer overflow")),
                    ("-", Value::Float(v)) => finite(-v, s),
                    ("+", v @ Value::Int(_)) | ("+", v @ Value::Float(_)) => Ok(v),
                    _ => Err(Error::at(s, format!("invalid unary '{op}'"))),
                }
            }
            ExprKind::Binary(op, a, b) => {
                let a = self.eval(a)?;
                if op == "&&" && !a.boolean(s)? {
                    return Ok(Value::Bool(false));
                }
                if op == "||" && a.boolean(s)? {
                    return Ok(Value::Bool(true));
                }
                let b = self.eval(b)?;
                binary(op, a, b, s)
            }
            ExprKind::Member(e, n) => {
                let v = self.eval(e)?;
                match &v {
                    Value::Object(o) => o
                        .get(n)
                        .cloned()
                        .ok_or_else(|| Error::at(s, format!("missing field '{n}'"))),
                    Value::Array(a) if n == "length" => Ok(Value::Int(a.len() as i64)),
                    Value::String(a) if n == "length" => Ok(Value::Int(a.chars().count() as i64)),
                    Value::Agent(_) if n == "run" => Ok(Value::Method(Box::new(v), n.clone())),
                    Value::Builtin(b) if ["json", "audio", "clock"].contains(&b.as_str()) => {
                        Ok(Value::Builtin(format!("{b}.{n}")))
                    }
                    Value::String(_) | Value::Array(_) => Ok(Value::Method(Box::new(v), n.clone())),
                    _ => Err(Error::at(s, format!("value has no member '{n}'"))),
                }
            }
            ExprKind::Index(e, i) => {
                let v = self.eval(e)?;
                let i = self.eval(i)?;
                index(v, i, s)
            }
            ExprKind::Call(f, args) => {
                let f = self.eval(f)?;
                let a = args
                    .iter()
                    .map(|e| self.eval(e))
                    .collect::<Result<Vec<_>>>()?;
                match f {
                    Value::Function(n) => self.function(&n, a, s),
                    Value::Tool(n) => self.tool(&n, a, s),
                    Value::Builtin(n) => self.builtin(&n, a, s),
                    Value::Method(v, n) => self.method(*v, &n, a, s),
                    _ => Err(Error::at(s, "value is not callable")),
                }
            }
        }
    }
    fn function(&mut self, n: &str, args: Vec<Value>, s: &Site) -> Result<Value> {
        let f = self.shared.compiled.program.functions[n].clone();
        arity(&f.params, args.len(), s)?;
        if self.depth >= self.shared.limits.call_depth {
            let mut e = Error::at(s, "call depth budget exhausted");
            e.fatal = true;
            return Err(e);
        }
        let mut child = Self {
            shared: self.shared.clone(),
            scopes: vec![BTreeMap::new()],
            depth: self.depth + 1,
        };
        child.bind_params(&f.params, args, s)?;
        let value = match child.stmt(&f.body)? {
            Flow::Return(v) => v,
            Flow::Next if f.returns == Type::named("void") => Value::Null,
            _ => {
                return Err(Error::at(
                    &f.site,
                    "function finished without a return value",
                ))
            }
        };
        ensure_type(&self.shared.compiled.program, &f.returns, &value, s)?;
        Ok(value)
    }
    pub(crate) fn bind_params(
        &mut self,
        params: &[Param],
        args: Vec<Value>,
        s: &Site,
    ) -> Result<Vec<Value>> {
        arity(params, args.len(), s)?;
        let mut args = args.into_iter();
        let mut bound = Vec::new();
        for p in params {
            let v = if let Some(v) = args.next() {
                v
            } else {
                self.eval(p.default.as_ref().unwrap())?
            };
            ensure_type(&self.shared.compiled.program, &p.ty, &v, s)?;
            self.bind(&p.name, Some(&p.ty), v.clone(), s)?;
            bound.push(v);
        }
        Ok(bound)
    }
    fn scoped(&mut self, s: &Stmt) -> Result<Flow> {
        self.scopes.push(BTreeMap::new());
        let r = self.stmt(s);
        self.scopes.pop();
        r
    }
    fn stmt(&mut self, s: &Stmt) -> Result<Flow> {
        self.tick(&s.site)?;
        match &s.kind {
            StmtKind::Block(items) => {
                self.scopes.push(BTreeMap::new());
                let result = (|| {
                    for s in items {
                        let r = self.stmt(s)?;
                        if !matches!(r, Flow::Next) {
                            return Ok(r);
                        }
                    }
                    Ok(Flow::Next)
                })();
                self.scopes.pop();
                result
            }
            StmtKind::Let(n, t, e) => {
                let v = self.eval(e)?;
                self.bind(n, t.as_ref(), v, &s.site)?;
                Ok(Flow::Next)
            }
            StmtKind::Assign(target, e) => {
                self.assign(target, e, None)?;
                Ok(Flow::Next)
            }
            StmtKind::Update(target, op, e) => {
                self.assign(target, e, Some(op))?;
                Ok(Flow::Next)
            }
            StmtKind::Expr(e) => {
                self.eval(e)?;
                Ok(Flow::Next)
            }
            StmtKind::If(c, b, other) => {
                if self.eval(c)?.boolean(&c.site)? {
                    self.scoped(b)
                } else if let Some(b) = other {
                    self.scoped(b)
                } else {
                    Ok(Flow::Next)
                }
            }
            StmtKind::While(c, b) => {
                while self.eval(c)?.boolean(&c.site)? {
                    match self.scoped(b)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        _ => {}
                    }
                }
                Ok(Flow::Next)
            }
            StmtKind::For(init, c, step, b) => {
                self.scopes.push(BTreeMap::new());
                let result = (|| {
                    self.stmt(init)?;
                    while self.eval(c)?.boolean(&c.site)? {
                        match self.scoped(b)? {
                            Flow::Break => break,
                            Flow::Return(v) => return Ok(Flow::Return(v)),
                            _ => {}
                        }
                        self.stmt(step)?;
                    }
                    Ok(Flow::Next)
                })();
                self.scopes.pop();
                result
            }
            StmtKind::Each(n, t, e, b) => {
                let values = match self.eval(e)? {
                    Value::Array(a) => a,
                    _ => return Err(Error::at(&e.site, "foreach requires array")),
                };
                for v in values {
                    self.scopes.push(BTreeMap::new());
                    let result = (|| {
                        self.bind(n, t.as_ref(), v, &s.site)?;
                        self.stmt(b)
                    })();
                    self.scopes.pop();
                    match result? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        _ => {}
                    }
                }
                Ok(Flow::Next)
            }
            StmtKind::Return(e) => Ok(Flow::Return(if let Some(e) = e {
                self.eval(e)?
            } else {
                Value::Null
            })),
            StmtKind::Break => Ok(Flow::Break),
            StmtKind::Continue => Ok(Flow::Continue),
            StmtKind::Throw(e) => {
                let v = self.eval(e)?;
                Err(Error::at(&e.site, v.display()))
            }
            StmtKind::Try(b, catch, finally) => {
                let mut result = self.scoped(b);
                if let Err(e) = &result {
                    if !e.fatal {
                        if let Some((name, b)) = catch {
                            let value = Value::Object(
                                [
                                    (String::from("message"), Value::String(e.message.clone())),
                                    ("file".into(), Value::String(e.site.file.clone())),
                                    ("line".into(), Value::Int(e.site.line as i64)),
                                ]
                                .into_iter()
                                .collect(),
                            );
                            self.scopes.push(BTreeMap::new());
                            result = self
                                .bind(name, Some(&Type::named("object")), value, &s.site)
                                .and_then(|_| self.stmt(b));
                            self.scopes.pop();
                        }
                    }
                }
                if let Some(b) = finally {
                    let final_result = self.scoped(b);
                    if !matches!(final_result, Ok(Flow::Next))
                        && !matches!(&result,Err(e) if e.fatal)
                    {
                        result = final_result;
                    }
                }
                result
            }
        }
    }
    fn assign(&mut self, target: &Expr, expression: &Expr, op: Option<&str>) -> Result<()> {
        let mut path = Vec::new();
        let name = self.assignment_path(target, &mut path)?;
        let scope_index = self
            .scopes
            .iter()
            .rposition(|s| s.contains_key(&name))
            .ok_or_else(|| Error::at(&target.site, "assignment requires local variable"))?;
        let binding = self.scopes[scope_index][&name].clone();
        let mut next = binding.value;
        let previous = if op.is_some() {
            let mut v = next.clone();
            for key in &path {
                v = index(v, key.clone(), &target.site)?;
            }
            Some(v)
        } else {
            None
        };
        let rhs = self.eval(expression)?;
        let value = if let Some(op) = op {
            binary(op, previous.unwrap(), rhs, &target.site)?
        } else {
            rhs
        };
        replace(&mut next, &path, value, &target.site)?;
        ensure_type(
            &self.shared.compiled.program,
            &binding.ty,
            &next,
            &target.site,
        )?;
        self.scopes[scope_index].get_mut(&name).unwrap().value = typed_task(next, &binding.ty);
        Ok(())
    }
    fn assignment_path(&mut self, e: &Expr, path: &mut Vec<Value>) -> Result<String> {
        match &e.kind {
            ExprKind::Name(n) => Ok(n.clone()),
            ExprKind::Member(e, n) => {
                let root = self.assignment_path(e, path)?;
                path.push(Value::String(n.clone()));
                Ok(root)
            }
            ExprKind::Index(e, i) => {
                let root = self.assignment_path(e, path)?;
                path.push(self.eval(i)?);
                Ok(root)
            }
            _ => Err(Error::at(&e.site, "invalid assignment target")),
        }
    }
    fn builtin(&mut self, n: &str, args: Vec<Value>, s: &Site) -> Result<Value> {
        match n {
            "log" => {
                count(&args, 1, s)?;
                self.shared.logs.lock().unwrap().push(args[0].display());
                emit(
                    &self.shared.host,
                    ExecutionEvent::Log {
                        text: args[0].display(),
                    },
                );
                Ok(Value::Null)
            }
            "env" => {
                if !(1..=2).contains(&args.len()) {
                    return Err(Error::at(s, "env expects name and optional default"));
                }
                let name = args[0].text(s)?;
                match std::env::var(name) {
                    Ok(v) => Ok(Value::String(v)),
                    Err(_) => Ok(args.get(1).cloned().unwrap_or(Value::String(String::new()))),
                }
            }
            "json.stringify" => {
                count(&args, 1, s)?;
                Ok(Value::String(args[0].to_json(s)?.to_string()))
            }
            "json.parse" => {
                count(&args, 1, s)?;
                serde_json::from_str(&args[0].text(s)?)
                    .map(Value::from_json)
                    .map_err(|e| Error::at(s, e.to_string()))
            }
            "string" => {
                count(&args, 1, s)?;
                Ok(Value::String(args[0].display()))
            }
            "int" => {
                count(&args, 1, s)?;
                match &args[0] {
                    Value::Int(v) => Ok(Value::Int(*v)),
                    Value::String(v) => v
                        .parse()
                        .map(Value::Int)
                        .map_err(|_| Error::at(s, "invalid integer")),
                    _ => Err(Error::at(s, "int expects integer or string")),
                }
            }
            "double" => {
                count(&args, 1, s)?;
                let n = match &args[0] {
                    Value::Int(v) => *v as f64,
                    Value::Float(v) => *v,
                    Value::String(v) => v.parse().map_err(|_| Error::at(s, "invalid double"))?,
                    _ => return Err(Error::at(s, "double expects number or string")),
                };
                finite(n, s)
            }
            "len" => {
                count(&args, 1, s)?;
                match &args[0] {
                    Value::Array(a) => Ok(Value::Int(a.len() as i64)),
                    Value::String(a) => Ok(Value::Int(a.chars().count() as i64)),
                    Value::Object(a) => Ok(Value::Int(a.len() as i64)),
                    _ => Err(Error::at(s, "len expects string, array or object")),
                }
            }
            "assert" => {
                if !(1..=2).contains(&args.len()) {
                    return Err(Error::at(
                        s,
                        "assert expects condition and optional message",
                    ));
                }
                if !args[0].boolean(s)? {
                    return Err(Error::at(
                        s,
                        args.get(1)
                            .map(Value::display)
                            .unwrap_or_else(|| "assertion failed".into()),
                    ));
                }
                Ok(Value::Null)
            }
            "range" => {
                count(&args, 2, s)?;
                let start = args[0].integer(s)?;
                let end = args[1].integer(s)?;
                if end
                    .checked_sub(start)
                    .is_none_or(|n| !(0..=100_000).contains(&n))
                {
                    return Err(Error::at(s, "range must contain 0..100000 elements"));
                }
                Ok(Value::Array((start..end).map(Value::Int).collect()))
            }
            "clock.now" => {
                count(&args, 0, s)?;
                Ok(Value::Int(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_err(|e| Error::at(s, e.to_string()))?
                        .as_secs() as i64,
                ))
            }
            "audio.load" => {
                count(&args, 1, s)?;
                crate::AudioInput::from_path(&self.shared.compiled.root, &args[0].text(s)?)
                    .map(|a| Value::Audio(Arc::new(a)))
                    .map_err(|e| Error::at(s, e.to_string()))
            }
            "record" => {
                count(&args, 1, s)?;
                let seconds = args[0].integer(s)?;
                if !(0..=3600).contains(&seconds) {
                    return Err(Error::at(
                        s,
                        "record seconds must be 0..3600; zero means until Enter",
                    ));
                }
                let _exclusive = self.shared.microphone.lock().unwrap();
                let duration = self
                    .shared
                    .host
                    .recording_seconds
                    .unwrap_or(if seconds == 0 {
                        None
                    } else {
                        Some(seconds as u64)
                    });
                if let Some(path) = &self.shared.host.save_recording {
                    let parent = path
                        .parent()
                        .filter(|p| !p.as_os_str().is_empty())
                        .unwrap_or_else(|| std::path::Path::new("."));
                    if !parent.is_dir() || path.is_dir() {
                        return Err(Error::at(
                            s,
                            "recording destination must be a file in an existing directory",
                        ));
                    }
                }
                let mut operation = Operation::begin(
                    &self.shared.host,
                    "recording",
                    "Microphone",
                    &duration
                        .map(|n| format!("{n} seconds"))
                        .unwrap_or_else(|| "until Enter".into()),
                    s,
                );
                let audio = (self.shared.host.recorder)(duration).map_err(|e| Error::at(s, e))?;
                if let Some(path) = &self.shared.host.save_recording {
                    audio
                        .save_wav(path)
                        .map_err(|e| Error::at(s, e.to_string()))?;
                }
                operation.success = true;
                Ok(Value::Audio(Arc::new(audio)))
            }
            _ => Err(Error::at(s, format!("unknown builtin '{n}'"))),
        }
    }
    fn method(&mut self, v: Value, n: &str, args: Vec<Value>, s: &Site) -> Result<Value> {
        if let Value::Agent(agent) = &v {
            if n == "run" {
                count(&args, 1, s)?;
                return self.agent(agent, args[0].clone(), s);
            }
        }
        match (v, n) {
            (Value::String(a), "trim") => {
                count(&args, 0, s)?;
                Ok(Value::String(a.trim().into()))
            }
            (Value::String(a), "startsWith" | "endsWith" | "contains") => {
                count(&args, 1, s)?;
                let b = args[0].text(s)?;
                Ok(Value::Bool(match n {
                    "startsWith" => a.starts_with(&b),
                    "endsWith" => a.ends_with(&b),
                    _ => a.contains(&b),
                }))
            }
            (Value::String(a), "split") => {
                count(&args, 1, s)?;
                Ok(Value::Array(
                    a.split(&args[0].text(s)?)
                        .map(|s| Value::String(s.into()))
                        .collect(),
                ))
            }
            (Value::Array(a), "join") => {
                count(&args, 1, s)?;
                Ok(Value::String(
                    a.iter()
                        .map(|v| v.text(s))
                        .collect::<Result<Vec<_>>>()?
                        .join(&args[0].text(s)?),
                ))
            }
            (Value::Array(mut a), "concat") => {
                count(&args, 1, s)?;
                if let Value::Array(b) = &args[0] {
                    a.extend(b.clone());
                    Ok(Value::Array(a))
                } else {
                    Err(Error::at(s, "concat expects array"))
                }
            }
            _ => Err(Error::at(s, format!("unknown method '{n}'"))),
        }
    }
}
fn typed_task(value: Value, ty: &Type) -> Value {
    match (value, ty) {
        (Value::Task(task, _), Type::Task(t)) => Value::Task(task, Some((**t).clone())),
        (value, _) => value,
    }
}
fn inferred(v: &Value) -> Type {
    Type::named(match v {
        Value::Null => "any",
        Value::Bool(_) => "bool",
        Value::Int(_) => "int",
        Value::Float(_) => "double",
        Value::String(_) => "string",
        Value::Array(_) => return Type::Array(Box::new(Type::named("any"))),
        Value::Audio(_) => "Audio",
        Value::Function(_) => "Function",
        Value::Agent(_) => "Agent",
        _ => "any",
    })
}
fn finite(v: f64, s: &Site) -> Result<Value> {
    if v.is_finite() {
        Ok(Value::Float(v))
    } else {
        Err(Error::at(s, "non-finite numeric result"))
    }
}
fn index(v: Value, i: Value, s: &Site) -> Result<Value> {
    match (v, i) {
        (Value::Object(o), Value::String(i)) => o
            .get(&i)
            .cloned()
            .ok_or_else(|| Error::at(s, format!("missing key '{i}'"))),
        (Value::Array(a), Value::Int(i)) if i >= 0 => a
            .get(i as usize)
            .cloned()
            .ok_or_else(|| Error::at(s, "array index out of bounds")),
        (Value::String(a), Value::Int(i)) if i >= 0 => a
            .chars()
            .nth(i as usize)
            .map(|c| Value::String(c.to_string()))
            .ok_or_else(|| Error::at(s, "string index out of bounds")),
        _ => Err(Error::at(s, "invalid index")),
    }
}
fn replace(root: &mut Value, path: &[Value], value: Value, s: &Site) -> Result<()> {
    if path.is_empty() {
        *root = value;
        return Ok(());
    }
    let child = match (root, &path[0]) {
        (Value::Object(o), Value::String(k)) => o
            .get_mut(k)
            .ok_or_else(|| Error::at(s, format!("missing field '{k}'")))?,
        (Value::Array(a), Value::Int(i)) if *i >= 0 => a
            .get_mut(*i as usize)
            .ok_or_else(|| Error::at(s, "array index out of bounds"))?,
        _ => return Err(Error::at(s, "invalid assignment path")),
    };
    replace(child, &path[1..], value, s)
}
fn binary(op: &str, a: Value, b: Value, s: &Site) -> Result<Value> {
    if ["==", "!="].contains(&op) {
        let equal = match (&a, &b) {
            (Value::Int(a), Value::Float(b)) => *a as f64 == *b,
            (Value::Float(a), Value::Int(b)) => *a == *b as f64,
            _ => a.to_json(s)? == b.to_json(s)?,
        };
        return Ok(Value::Bool(if op == "==" { equal } else { !equal }));
    }
    if op == "&&" || op == "||" {
        return Ok(Value::Bool(if op == "&&" {
            a.boolean(s)? && b.boolean(s)?
        } else {
            a.boolean(s)? || b.boolean(s)?
        }));
    }
    if let (Value::String(a), Value::String(b)) = (&a, &b) {
        return match op {
            "+" => Ok(Value::String(format!("{a}{b}"))),
            "<" => Ok(Value::Bool(a < b)),
            "<=" => Ok(Value::Bool(a <= b)),
            ">" => Ok(Value::Bool(a > b)),
            ">=" => Ok(Value::Bool(a >= b)),
            _ => Err(Error::at(s, "invalid string operator")),
        };
    }
    if let (Value::Int(a), Value::Int(b)) = (&a, &b) {
        return match op {
            "+" => a.checked_add(*b),
            "-" => a.checked_sub(*b),
            "*" => a.checked_mul(*b),
            "/" => a.checked_div(*b),
            "%" => a.checked_rem(*b),
            "<" => return Ok(Value::Bool(a < b)),
            "<=" => return Ok(Value::Bool(a <= b)),
            ">" => return Ok(Value::Bool(a > b)),
            ">=" => return Ok(Value::Bool(a >= b)),
            _ => None,
        }
        .map(Value::Int)
        .ok_or_else(|| Error::at(s, "integer overflow, zero divisor or invalid operator"));
    }
    let number = |v: &Value| match v {
        Value::Int(v) => Ok(*v as f64),
        Value::Float(v) => Ok(*v),
        _ => Err(Error::at(
            s,
            "arithmetic requires numbers of compatible types",
        )),
    };
    let a = number(&a)?;
    let b = number(&b)?;
    match op {
        "+" => finite(a + b, s),
        "-" => finite(a - b, s),
        "*" => finite(a * b, s),
        "/" => finite(a / b, s),
        "%" => finite(a % b, s),
        "<" => Ok(Value::Bool(a < b)),
        "<=" => Ok(Value::Bool(a <= b)),
        ">" => Ok(Value::Bool(a > b)),
        ">=" => Ok(Value::Bool(a >= b)),
        _ => Err(Error::at(s, "invalid numeric operator")),
    }
}
