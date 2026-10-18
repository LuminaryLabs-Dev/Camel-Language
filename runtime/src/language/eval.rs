use super::{
    check::{arity, BUILTINS},
    host::{count, emit, Capability, ExecutionEvent, Host, Operation},
    syntax::*,
    value::{drain, ensure_type, wait, TaskState, Value},
    CompiledProgram,
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
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
    pub tool_calls: usize,
    pub tokens: usize,
    pub deadline: std::time::Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            steps: 100_000,
            call_depth: 128,
            tasks: 64,
            tool_calls: 256,
            tokens: 32_768,
            deadline: std::time::Duration::from_secs(600),
        }
    }
}
pub struct Execution {
    pub value: Value,
    pub logs: Vec<String>,
    pub tokens_used: usize,
    pub tool_calls_used: usize,
    pub source_fingerprint: String,
    pub cassette_fingerprint: Option<String>,
}
pub struct Engine {
    compiled: CompiledProgram,
    host: Host,
    limits: Limits,
    checkpoint_path: Option<PathBuf>,
    resume_path: Option<PathBuf>,
}
impl Engine {
    pub fn new(compiled: CompiledProgram, host: Host) -> Self {
        Self {
            compiled,
            host,
            limits: Limits::default(),
            checkpoint_path: None,
            resume_path: None,
        }
    }
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }
    /// Persist state at declared workflow checkpoint statements.
    pub fn with_checkpoint_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.checkpoint_path = Some(path.into());
        self
    }
    /// Resume at the statement immediately after the saved checkpoint.
    pub fn resume_from(mut self, path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        self.checkpoint_path = Some(path.clone());
        self.resume_path = Some(path);
        self
    }
    pub fn run(&self, entry: &str, args: Vec<Value>) -> Result<Execution> {
        self.compiled.entry_parameters(entry)?;
        let fingerprint = self.compiled.fingerprint();
        let resume = self
            .resume_path
            .as_ref()
            .map(|path| {
                let bytes = std::fs::read(path).map_err(|e| {
                    Error::at(
                        &self.compiled.program.functions[entry].site,
                        format!("cannot read checkpoint: {e}"),
                    )
                })?;
                let state: Checkpoint = serde_json::from_slice(&bytes).map_err(|e| {
                    Error::at(
                        &self.compiled.program.functions[entry].site,
                        format!("invalid checkpoint: {e}"),
                    )
                })?;
                if state.version != 1 {
                    return Err(Error::at(
                        &self.compiled.program.functions[entry].site,
                        format!("unsupported checkpoint version {}", state.version),
                    ));
                }
                if state.entry != entry || state.fingerprint != fingerprint {
                    return Err(Error::at(
                        &self.compiled.program.functions[entry].site,
                        "checkpoint does not match this workflow source",
                    ));
                }
                Ok(state)
            })
            .transpose()?;
        let site = self.compiled.program.functions[entry].site.clone();
        if let Some(cassette) = &self.host.cassette {
            cassette
                .bind_source(&fingerprint)
                .map_err(|message| Error::at(&site, message))?;
        }
        if let (Some(cassette), Some(checkpoint)) = (&self.host.cassette, &resume) {
            cassette
                .seek(checkpoint.cassette_cursor)
                .map_err(|message| Error::at(&site, message))?;
        }
        let steps_used = resume.as_ref().map_or(0, |state| state.steps_used);
        let tokens_used = resume.as_ref().map_or(0, |state| state.tokens_used);
        let tool_calls_used = resume.as_ref().map_or(0, |state| state.tool_calls_used);
        if steps_used > self.limits.steps
            || tokens_used > self.limits.tokens
            || tool_calls_used > self.limits.tool_calls
        {
            return Err(Error::at(
                &site,
                "checkpoint already exceeds one or more configured run budgets",
            ));
        }
        let elapsed_before =
            std::time::Duration::from_millis(resume.as_ref().map_or(0, |state| state.elapsed_ms));
        let mut operation = Operation::begin(&self.host, "workflow", entry, "", &site);
        let shared = Arc::new(Shared {
            compiled: self.compiled.clone(),
            host: self.host.clone(),
            limits: self.limits.clone(),
            fuel: AtomicUsize::new(self.limits.steps - steps_used),
            tasks: Mutex::new(Vec::new()),
            logs: Mutex::new(
                resume
                    .as_ref()
                    .map_or_else(Vec::new, |state| state.logs.clone()),
            ),
            microphone: Mutex::new(()),
            started: std::time::Instant::now(),
            elapsed_before,
            tool_calls: AtomicUsize::new(tool_calls_used),
            tokens: AtomicUsize::new(tokens_used),
            entry: entry.into(),
            fingerprint,
            checkpoint_path: self.checkpoint_path.clone(),
            resume: Mutex::new(resume),
        });
        let mut ctx = Context {
            shared: shared.clone(),
            scopes: vec![BTreeMap::new()],
            depth: 0,
            checkpoint_block: false,
        };
        ctx.emit_budget();
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
        if result.is_ok() {
            if let Some(cassette) = &self.host.cassette {
                if let Err(message) = cassette.ensure_consumed() {
                    result = Err(Error::at(&site, message));
                }
            }
        }
        operation.success = result.is_ok();
        Ok(Execution {
            value: result?,
            logs,
            tokens_used: shared.tokens.load(Ordering::Acquire),
            tool_calls_used: shared.tool_calls.load(Ordering::Acquire),
            source_fingerprint: self.compiled.fingerprint(),
            cassette_fingerprint: self.host.cassette.as_ref().map(|c| c.fingerprint()),
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
    started: std::time::Instant,
    elapsed_before: std::time::Duration,
    tool_calls: AtomicUsize,
    tokens: AtomicUsize,
    entry: String,
    fingerprint: String,
    checkpoint_path: Option<PathBuf>,
    resume: Mutex<Option<Checkpoint>>,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Checkpoint {
    version: u32,
    entry: String,
    fingerprint: String,
    label: String,
    next_statement: usize,
    scopes: Vec<BTreeMap<String, SavedBinding>>,
    logs: Vec<String>,
    steps_used: usize,
    tokens_used: usize,
    tool_calls_used: usize,
    elapsed_ms: u64,
    cassette_cursor: usize,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct SavedBinding {
    ty: Type,
    value: serde_json::Value,
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
    checkpoint_block: bool,
}
#[derive(Clone)]
enum Flow {
    Next,
    Return(Value),
    Break,
    Continue,
}
impl Context {
    fn elapsed(&self) -> std::time::Duration {
        self.shared
            .elapsed_before
            .saturating_add(self.shared.started.elapsed())
    }
    pub(crate) fn global_context(&self) -> Self {
        Self {
            shared: self.shared.clone(),
            scopes: vec![BTreeMap::new()],
            depth: self.depth,
            checkpoint_block: false,
        }
    }
    pub(crate) fn tick(&self, site: &Site) -> Result<()> {
        let fail = if self.shared.host.cancellation.is_cancelled() {
            Some("execution cancelled")
        } else if self.elapsed() >= self.shared.limits.deadline {
            Some("execution deadline exceeded")
        } else {
            None
        };
        if let Some(message) = fail {
            let mut error = Error::at(site, message);
            error.fatal = true;
            return Err(error);
        }
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
    pub(crate) fn reserve_tool_call(&self, site: &Site) -> Result<()> {
        reserve_budget(
            &self.shared.tool_calls,
            self.shared.limits.tool_calls,
            1,
            site,
            "tool-call budget exhausted",
        )?;
        self.emit_budget();
        Ok(())
    }
    pub(crate) fn record_tokens(&self, count: usize, site: &Site) -> Result<()> {
        reserve_budget(
            &self.shared.tokens,
            self.shared.limits.tokens,
            count,
            site,
            "token budget exhausted",
        )?;
        self.emit_budget();
        Ok(())
    }
    pub(crate) fn remaining_tokens(&self) -> usize {
        self.shared
            .limits
            .tokens
            .saturating_sub(self.shared.tokens.load(Ordering::Acquire))
    }
    fn emit_budget(&self) {
        let millis =
            |duration: std::time::Duration| duration.as_millis().min(u64::MAX as u128) as u64;
        emit(
            &self.shared.host,
            ExecutionEvent::Budget {
                tokens_used: self.shared.tokens.load(Ordering::Acquire),
                tokens_limit: self.shared.limits.tokens,
                tool_calls_used: self.shared.tool_calls.load(Ordering::Acquire),
                tool_calls_limit: self.shared.limits.tool_calls,
                elapsed_ms: millis(self.elapsed()),
                deadline_ms: millis(self.shared.limits.deadline),
            },
        );
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
            checkpoint_block: n == self.shared.entry,
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
                let checkpoint_block = std::mem::take(&mut self.checkpoint_block);
                self.scopes.push(BTreeMap::new());
                let mut start = 0;
                if checkpoint_block {
                    if let Some(checkpoint) = self.shared.resume.lock().unwrap().take() {
                        if checkpoint.next_statement == 0
                            || checkpoint.next_statement > items.len()
                            || !matches!(
                                items.get(checkpoint.next_statement - 1).map(|s| &s.kind),
                                Some(StmtKind::Checkpoint(label)) if label == &checkpoint.label
                            )
                        {
                            self.scopes.pop();
                            return Err(Error::at(&s.site, "checkpoint position is invalid"));
                        }
                        if checkpoint.scopes.len() != 2 {
                            self.scopes.pop();
                            return Err(Error::at(&s.site, "checkpoint scope stack is invalid"));
                        }
                        let mut restored = Vec::with_capacity(checkpoint.scopes.len());
                        for scope in checkpoint.scopes {
                            let mut bindings = BTreeMap::new();
                            for (name, binding) in scope {
                                let value = Value::from_json(binding.value);
                                ensure_type(
                                    &self.shared.compiled.program,
                                    &binding.ty,
                                    &value,
                                    &s.site,
                                )?;
                                bindings.insert(
                                    name,
                                    Binding {
                                        value,
                                        ty: binding.ty,
                                    },
                                );
                            }
                            restored.push(bindings);
                        }
                        self.scopes = restored;
                        start = checkpoint.next_statement;
                    }
                }
                let result = (|| {
                    for (index, s) in items.iter().enumerate().skip(start) {
                        if let StmtKind::Checkpoint(label) = &s.kind {
                            if !checkpoint_block {
                                return Err(Error::at(
                                    &s.site,
                                    "checkpoint must be a direct statement in the entry workflow",
                                ));
                            }
                            self.save_checkpoint(label, index + 1, &s.site)?;
                            continue;
                        }
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
            StmtKind::Checkpoint(_) => Err(Error::at(
                &s.site,
                "checkpoint must be a direct statement in the entry workflow",
            )),
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
    fn save_checkpoint(&self, label: &str, next_statement: usize, site: &Site) -> Result<()> {
        let path =
            self.shared.checkpoint_path.as_ref().ok_or_else(|| {
                Error::at(site, "checkpoint statement requires --checkpoint <path>")
            })?;
        if self
            .shared
            .tasks
            .lock()
            .unwrap()
            .iter()
            .any(|task| !task.2.load(Ordering::Acquire))
        {
            return Err(Error::at(
                site,
                "checkpoint requires spawned tasks to finish or be awaited",
            ));
        }
        let scopes = self
            .scopes
            .iter()
            .map(|scope| {
                scope
                    .iter()
                    .map(|(name, binding)| {
                        Ok((
                            name.clone(),
                            SavedBinding {
                                ty: binding.ty.clone(),
                                value: binding.value.to_json(site)?,
                            },
                        ))
                    })
                    .collect::<Result<BTreeMap<_, _>>>()
            })
            .collect::<Result<Vec<_>>>()?;
        let checkpoint = Checkpoint {
            version: 1,
            entry: self.shared.entry.clone(),
            fingerprint: self.shared.fingerprint.clone(),
            label: label.into(),
            next_statement,
            scopes,
            logs: self.shared.logs.lock().unwrap().clone(),
            steps_used: self.shared.limits.steps - self.shared.fuel.load(Ordering::Acquire),
            tokens_used: self.shared.tokens.load(Ordering::Acquire),
            tool_calls_used: self.shared.tool_calls.load(Ordering::Acquire),
            elapsed_ms: self.elapsed().as_millis().min(u64::MAX as u128) as u64,
            cassette_cursor: self
                .shared
                .host
                .cassette
                .as_ref()
                .map_or(0, |cassette| cassette.cursor()),
        };
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        let mut pending = tempfile::NamedTempFile::new_in(parent)
            .map_err(|e| Error::at(site, format!("cannot create checkpoint: {e}")))?;
        serde_json::to_writer_pretty(&mut pending, &checkpoint)
            .map_err(|e| Error::at(site, format!("cannot encode checkpoint: {e}")))?;
        pending
            .as_file()
            .sync_all()
            .map_err(|e| Error::at(site, format!("cannot sync checkpoint: {e}")))?;
        pending
            .persist(path)
            .map_err(|e| Error::at(site, format!("cannot save checkpoint: {}", e.error)))?;
        Ok(())
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
                let default = args.get(1).cloned().unwrap_or(Value::String(String::new()));
                if let Some(cassette) = &self.shared.host.cassette {
                    let recorded = cassette
                        .exchange(
                            "environment",
                            serde_json::json!({"name":name,"default":default.to_json(s)?}),
                            || {
                                Ok(std::env::var(&name)
                                    .map(serde_json::Value::String)
                                    .unwrap_or(default.to_json(s).map_err(|e| e.to_string())?))
                            },
                        )
                        .map_err(|error| Error::at(s, error))?;
                    Ok(Value::from_json(recorded))
                } else {
                    Ok(Value::String(
                        std::env::var(name).unwrap_or_else(|_| default.display()),
                    ))
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
                let now = || -> Result<i64> {
                    Ok(std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_err(|e| Error::at(s, e.to_string()))?
                        .as_secs() as i64)
                };
                if let Some(cassette) = &self.shared.host.cassette {
                    let value = cassette
                        .exchange("clock", serde_json::Value::Null, || {
                            now()
                                .map(serde_json::Value::from)
                                .map_err(|e| e.to_string())
                        })
                        .map_err(|e| Error::at(s, e))?;
                    Ok(Value::from_json(value))
                } else {
                    now().map(Value::Int)
                }
            }
            "audio.load" => {
                count(&args, 1, s)?;
                if !self
                    .shared
                    .host
                    .cassette
                    .as_ref()
                    .is_some_and(|c| c.is_replay())
                {
                    self.shared.host.require_approval(Capability::FileRead, s)?;
                }
                let requested = args[0].text(s)?;
                if let Some(cassette) = &self.shared.host.cassette {
                    let value = cassette
                        .exchange("audio_load", serde_json::Value::String(requested.clone()), || {
                            let audio = crate::AudioInput::from_path(&self.shared.compiled.root, &requested)
                                .map_err(|e| e.to_string())?;
                            Ok(serde_json::json!({"format":audio.format,"data":audio.data_base64}))
                        })
                        .map_err(|e| Error::at(s, e))?;
                    let audio = crate::AudioInput {
                        path: self.shared.compiled.root.join(requested),
                        format: value["format"].as_str().unwrap_or("wav").into(),
                        data_base64: value["data"].as_str().unwrap_or_default().into(),
                    };
                    Ok(Value::Audio(Arc::new(audio)))
                } else {
                    crate::AudioInput::from_path(&self.shared.compiled.root, &requested)
                        .map(|a| Value::Audio(Arc::new(a)))
                        .map_err(|e| Error::at(s, e.to_string()))
                }
            }
            "record" => {
                count(&args, 1, s)?;
                if !self
                    .shared
                    .host
                    .cassette
                    .as_ref()
                    .is_some_and(|c| c.is_replay())
                {
                    self.shared
                        .host
                        .require_approval(Capability::Microphone, s)?;
                }
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
                    if !self
                        .shared
                        .host
                        .cassette
                        .as_ref()
                        .is_some_and(|c| c.is_replay())
                    {
                        self.shared
                            .host
                            .require_approval(Capability::FileWrite, s)?;
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
                let audio = if let Some(cassette) = &self.shared.host.cassette {
                    let value = cassette
                        .exchange("microphone", serde_json::json!({"seconds":duration}), || {
                            let audio = (self.shared.host.recorder)(duration)?;
                            Ok(serde_json::json!({"format":audio.format,"data":audio.data_base64}))
                        })
                        .map_err(|e| Error::at(s, e))?;
                    crate::AudioInput {
                        path: std::path::PathBuf::new(),
                        format: value["format"].as_str().unwrap_or("wav").into(),
                        data_base64: value["data"].as_str().unwrap_or_default().into(),
                    }
                } else {
                    (self.shared.host.recorder)(duration).map_err(|e| Error::at(s, e))?
                };
                if !self
                    .shared
                    .host
                    .cassette
                    .as_ref()
                    .is_some_and(|c| c.is_replay())
                {
                    if let Some(path) = &self.shared.host.save_recording {
                        audio
                            .save_wav(path)
                            .map_err(|e| Error::at(s, e.to_string()))?;
                    }
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
fn reserve_budget(
    counter: &AtomicUsize,
    limit: usize,
    amount: usize,
    site: &Site,
    message: &str,
) -> Result<()> {
    let mut used = counter.load(Ordering::Acquire);
    loop {
        let Some(next) = used.checked_add(amount).filter(|next| *next <= limit) else {
            let mut error = Error::at(site, message);
            error.fatal = true;
            return Err(error);
        };
        match counter.compare_exchange_weak(used, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return Ok(()),
            Err(observed) => used = observed,
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
