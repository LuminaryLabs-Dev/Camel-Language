use super::syntax::{Error, Program, Result, Site, Type};
use crate::AudioInput;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
};
pub(crate) type TaskState = Arc<(Mutex<Option<Result<Value>>>, Condvar, AtomicBool)>;
#[derive(Clone)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
    Audio(Arc<AudioInput>),
    Function(String),
    Agent(String),
    Tool(String),
    Reference(String),
    Builtin(String),
    Method(Box<Value>, String),
    Task(TaskState, Option<Type>),
}
impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display())
    }
}
impl Value {
    pub fn from_json(v: serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(v) => Self::Bool(v),
            serde_json::Value::Number(v) => {
                if let Some(i) = v.as_i64() {
                    Self::Int(i)
                } else {
                    Self::Float(v.as_f64().unwrap())
                }
            }
            serde_json::Value::String(v) => Self::String(v),
            serde_json::Value::Array(v) => {
                Self::Array(v.into_iter().map(Self::from_json).collect())
            }
            serde_json::Value::Object(v) => Self::Object(
                v.into_iter()
                    .map(|(k, v)| (k, Self::from_json(v)))
                    .collect(),
            ),
        }
    }
    pub fn to_json(&self, site: &Site) -> Result<serde_json::Value> {
        Ok(match self {
            Self::Null => serde_json::Value::Null,
            Self::Bool(v) => (*v).into(),
            Self::Int(v) => (*v).into(),
            Self::Float(v) => serde_json::Number::from_f64(*v)
                .ok_or_else(|| Error::at(site, "non-finite number"))?
                .into(),
            Self::String(v) => v.clone().into(),
            Self::Array(v) => {
                serde_json::Value::Array(v.iter().map(|v| v.to_json(site)).collect::<Result<_>>()?)
            }
            Self::Object(v) => serde_json::Value::Object(
                v.iter()
                    .map(|(k, v)| Ok((k.clone(), v.to_json(site)?)))
                    .collect::<Result<_>>()?,
            ),
            _ => {
                return Err(Error::at(
                    site,
                    "cannot serialize audio, tasks or callable values",
                ))
            }
        })
    }
    pub fn display(&self) -> String {
        match self {
            Self::String(s) => s.clone(),
            Self::Function(s)
            | Self::Agent(s)
            | Self::Tool(s)
            | Self::Reference(s)
            | Self::Builtin(s) => format!("<{s}>"),
            Self::Audio(a) => format!("<audio:{}>", a.path.display()),
            Self::Task(..) => "<task>".into(),
            Self::Method(_, n) => format!("<method:{n}>"),
            _ => self
                .to_json(&Site {
                    file: "value".into(),
                    line: 1,
                    column: 1,
                    offset: 0,
                    end_line: 1,
                    end_column: 1,
                    end_offset: 0,
                })
                .map(|v| v.to_string())
                .unwrap_or_else(|_| "<value>".into()),
        }
    }
    pub(crate) fn boolean(&self, s: &Site) -> Result<bool> {
        if let Self::Bool(v) = self {
            Ok(*v)
        } else {
            Err(Error::at(s, "expected bool"))
        }
    }
    pub(crate) fn integer(&self, s: &Site) -> Result<i64> {
        if let Self::Int(v) = self {
            Ok(*v)
        } else {
            Err(Error::at(s, "expected int"))
        }
    }
    pub(crate) fn text(&self, s: &Site) -> Result<String> {
        if let Self::String(v) = self {
            Ok(v.clone())
        } else {
            Err(Error::at(s, "expected string"))
        }
    }
}
pub(crate) fn ensure_type(p: &Program, ty: &Type, v: &Value, site: &Site) -> Result<()> {
    ensure_depth(p, ty, v, site, 0)
}
fn ensure_depth(p: &Program, ty: &Type, v: &Value, site: &Site, depth: usize) -> Result<()> {
    if depth > 64 {
        return Err(Error::at(site, "value type nesting exceeds 64"));
    }
    let valid = match (ty, v) {
        (Type::Array(t), Value::Array(values)) => {
            for v in values {
                ensure_depth(p, t, v, site, depth + 1)?;
            }
            true
        }
        (Type::Task(_), Value::Task(..)) => true,
        (Type::Named(n), _) => match n.as_str() {
            "any" => true,
            "void" => matches!(v, Value::Null),
            "int" => matches!(v, Value::Int(_)),
            "double" => matches!(v, Value::Int(_) | Value::Float(_)),
            "bool" => matches!(v, Value::Bool(_)),
            "string" => matches!(v, Value::String(_)),
            "Audio" => matches!(v, Value::Audio(_)),
            "Agent" => matches!(v, Value::Agent(_)),
            "Function" => matches!(v, Value::Function(_)),
            "Tool" => matches!(v, Value::Tool(_)),
            "object" => matches!(v, Value::Object(_)),
            "AgentResult" => {
                matches!(v,Value::Object(o) if o.contains_key("data") && matches!(o.get("text"),Some(Value::String(_))))
            }
            _ => {
                if let (Some(fields), Value::Object(o)) = (p.records.get(n), v) {
                    if fields.keys().ne(o.keys()) {
                        return Err(Error::at(
                            site,
                            format!("record '{n}' requires exactly its declared fields"),
                        ));
                    }
                    for (name, t) in fields {
                        ensure_depth(p, t, &o[name], site, depth + 1).map_err(|mut e| {
                            e.message = format!("field '{name}': {}", e.message);
                            e
                        })?;
                    }
                    true
                } else {
                    false
                }
            }
        },
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(Error::at(
            site,
            format!("expected {ty}, got {}", v.display()),
        ))
    }
}
pub(crate) fn wait(task: &TaskState) -> Result<Value> {
    task.2.store(true, Ordering::Release);
    drain(task)
}
pub(crate) fn drain(task: &TaskState) -> Result<Value> {
    let (lock, cv, _) = &**task;
    let mut result = lock.lock().unwrap_or_else(|e| e.into_inner());
    while result.is_none() {
        result = cv.wait(result).unwrap_or_else(|e| e.into_inner());
    }
    result.as_ref().unwrap().clone()
}
