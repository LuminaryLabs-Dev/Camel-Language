use super::{
    eval::Context,
    host::{Capability, Operation},
    syntax::*,
    value::{ensure_type, Value},
};
use serde_json::{json, Value as Json};
use std::collections::{BTreeMap, BTreeSet};

impl Context {
    fn configuration(&mut self, c: &BTreeMap<String, Expr>) -> Result<BTreeMap<String, Value>> {
        let mut ctx = self.global_context();
        c.iter()
            .map(|(k, e)| Ok((k.clone(), ctx.eval(e)?)))
            .collect()
    }
    pub(crate) fn tool(&mut self, name: &str, args: Vec<Value>, site: &Site) -> Result<Value> {
        self.tick(site)?;
        self.reserve_tool_call(site)?;
        let t = self.shared.compiled.program.tools[name].clone();
        let mut ctx = self.global_context();
        let args = ctx.bind_params(&t.params, args, site)?;
        let c = self.configuration(&t.config)?;
        let handler = required_text(&c, "handler", site)?;
        let capability = self.shared.host.tools.capability(&handler, site)?;
        if !self
            .shared
            .host
            .cassette
            .as_ref()
            .is_some_and(|c| c.is_replay())
        {
            self.shared.host.require_approval(capability, site)?;
        }
        let mut operation = Operation::begin(&self.shared.host, "tool", name, &handler, site);
        let value = if let Some(cassette) = &self.shared.host.cassette {
            let input = json!({
                "handler": handler,
                "args": args.iter().map(|value| value.to_json(site)).collect::<Result<Vec<_>>>()?
            });
            let result = cassette
                .exchange("tool", input, || {
                    self.shared
                        .host
                        .tools
                        .invoke(&handler, &args, &self.shared.compiled.root, site)
                        .and_then(|value| value.to_json(site))
                        .map_err(|error| error.to_string())
                })
                .map_err(|error| Error::at(site, error))?;
            Value::from_json(result)
        } else {
            self.shared
                .host
                .tools
                .invoke(&handler, &args, &self.shared.compiled.root, site)?
        };
        ensure_type(&self.shared.compiled.program, &t.returns, &value, site)?;
        operation.success = true;
        Ok(value)
    }
    pub(crate) fn agent(&mut self, name: &str, prompt: Value, site: &Site) -> Result<Value> {
        let cassette = self.shared.host.cassette.clone();
        if !cassette.as_ref().is_some_and(|c| c.is_replay()) {
            self.shared.host.require_approval(Capability::Model, site)?;
        }
        let provider = self.shared.host.provider.clone();
        if provider.is_none() && !cassette.as_ref().is_some_and(|c| c.is_replay()) {
            return Err(Error::at(site, "no model provider configured"));
        }
        let provider_capabilities = provider.as_ref().map(|p| p.capabilities()).unwrap_or(
            super::host::ProviderCapabilities {
                tool_calls: true,
                structured_output: true,
                token_usage: true,
            },
        );
        let declaration = self.shared.compiled.program.agents[name].clone();
        let c = self.configuration(&declaration)?;
        let model = match self.shared.host.agent_models.get(name) {
            Some(model) if !model.trim().is_empty() => model.clone(),
            _ => required_text(&c, "model", site)?,
        };
        if model.trim().is_empty() {
            return Err(Error::at(site, "agent model cannot be empty"));
        }
        let turns = bounded_int(&c, "maxTurns", 4, 1, 128, site)?;
        let retries = bounded_int(&c, "retries", 0, 0, 3, site)?;
        let temperature = match c.get("temperature") {
            Some(Value::Float(t)) => *t,
            Some(Value::Int(t)) => *t as f64,
            None => 0.0,
            _ => return Err(Error::at(site, "temperature must be numeric")),
        };
        if !(0.0..=2.0).contains(&temperature) {
            return Err(Error::at(site, "temperature must be 0..2"));
        }
        let policy = if let Some(Value::Reference(n)) = c.get("policy") {
            let p = self.shared.compiled.program.policies[n].clone();
            self.configuration(&p)?
        } else {
            BTreeMap::new()
        };
        // Validate every capability setting before making any provider request.
        for key in [
            "allowNetwork",
            "allowFileRead",
            "allowFileWrite",
            "allowShell",
        ] {
            flag(&policy, key, site)?;
        }
        let max_tools = bounded_int(&policy, "maxToolCalls", 0, 0, 1024, site)?;
        let offered = names(c.get("tools"), site)?;
        let allowed = names(policy.get("allowTools"), site)?;
        let response = match c.get("response") {
            Some(Value::Reference(n)) => Some(n.clone()),
            None => None,
            _ => return Err(Error::at(site, "response must be a record")),
        };
        let mut system = match c.get("system") {
            Some(v) => v.text(site)?,
            None => String::new(),
        };
        if let Some(n) = &response {
            let schema = schema(&self.shared.compiled.program, &Type::named(n), site, 0)?;
            system.push_str(&format!(
                "\nReturn only JSON matching this schema, with no additional properties: {schema}"
            ));
        }
        let mut definitions = Vec::new();
        for tool in &offered {
            let t = self.shared.compiled.program.tools[tool].clone();
            let cfg = self.configuration(&t.config)?;
            let handler = required_text(&cfg, "handler", site)?;
            let capability = self.shared.host.tools.capability(&handler, site)?;
            if !cassette.as_ref().is_some_and(|c| c.is_replay()) {
                self.shared.host.require_approval(capability, site)?;
            }
            authorize(tool, capability, &policy, &allowed, site)?;
            if max_tools == 0 {
                return Err(Error::at(site, "policy prohibits all tool calls"));
            }
            let mut properties = serde_json::Map::new();
            for p in &t.params {
                properties.insert(
                    p.name.clone(),
                    schema(&self.shared.compiled.program, &p.ty, site, 0)?,
                );
            }
            let required: Vec<&str> = t
                .params
                .iter()
                .filter(|p| p.default.is_none())
                .map(|p| p.name.as_str())
                .collect();
            definitions.push(json!({"type":"function","function":{"name":tool,"description":cfg.get("description").map(Value::display).unwrap_or_default(),"parameters":{"type":"object","properties":properties,"required":required,"additionalProperties":false}}}));
        }
        if !definitions.is_empty() && !provider_capabilities.tool_calls {
            return Err(Error::at(
                site,
                "provider does not support tool calls required by this agent",
            ));
        }
        if response.is_some() && !provider_capabilities.structured_output {
            return Err(Error::at(
                site,
                "provider does not support structured output required by this agent",
            ));
        }
        if !provider_capabilities.token_usage {
            return Err(Error::at(
                site,
                "provider does not report token usage required by the run budget",
            ));
        }
        let mut messages = vec![
            json!({"role":"system","content":system}),
            user_message(prompt, site)?,
        ];
        let mut tool_count = 0;
        let mut used_tools = Vec::new();
        let mut operation = Operation::begin(&self.shared.host, "agent", name, &model, site);
        let mut ids = BTreeSet::new();
        for turn in 0..turns {
            let mut assistant = None;
            for attempt in 0..=retries {
                self.tick(site)?;
                let mut payload = json!({"model":model,"messages":messages,"temperature":temperature,"stream":false});
                payload["max_tokens"] = self.remaining_tokens().into();
                if !definitions.is_empty() {
                    payload["tools"] = definitions.clone().into();
                }
                let mut request = Operation::begin(
                    &self.shared.host,
                    "model",
                    name,
                    &format!("turn {}, attempt {}", turn + 1, attempt + 1),
                    site,
                );
                let completion = if let Some(cassette) = &cassette {
                    cassette.exchange("model", payload.clone(), || {
                        provider
                            .as_ref()
                            .ok_or_else(|| "no model provider configured".to_string())?
                            .complete(payload)
                    })
                } else {
                    provider
                        .as_ref()
                        .expect("provider checked above")
                        .complete(payload)
                };
                match completion {
                    Ok(v) => {
                        request.success = true;
                        let tokens = v
                            .pointer("/usage/total_tokens")
                            .and_then(Json::as_u64)
                            .and_then(|count| usize::try_from(count).ok())
                            .ok_or_else(|| Error::at(site, "provider advertised token usage but omitted usage.total_tokens"))?;
                        self.record_tokens(tokens, site)?;
                        assistant = Some(v);
                        break;
                    }
                    Err(e) if attempt == retries => {
                        return Err(Error::at(site, format!("provider: {e}")))
                    }
                    Err(_) => {}
                }
            }
            let mut assistant = assistant.unwrap();
            let calls = match assistant.get("tool_calls") {
                Some(Json::Array(a)) => a.clone(),
                Some(Json::Null) | None => Vec::new(),
                _ => return Err(Error::at(site, "provider tool_calls must be an array")),
            };
            if !calls.is_empty() {
                if tool_count + calls.len() > max_tools {
                    return Err(Error::at(site, "agent tool-call budget exhausted"));
                }
                let mut planned = Vec::new();
                for call in &calls {
                    let tool = call
                        .pointer("/function/name")
                        .and_then(Json::as_str)
                        .ok_or_else(|| Error::at(site, "tool call missing function name"))?;
                    if !offered.contains(tool) {
                        return Err(Error::at(site, format!("tool '{tool}' was not offered")));
                    }
                    let id = call
                        .get("id")
                        .and_then(Json::as_str)
                        .ok_or_else(|| Error::at(site, "tool call missing id"))?;
                    if !ids.insert(id.to_string()) {
                        return Err(Error::at(site, "duplicate tool call id"));
                    }
                    let raw = call
                        .pointer("/function/arguments")
                        .and_then(Json::as_str)
                        .ok_or_else(|| Error::at(site, "tool call arguments must be JSON text"))?;
                    let args: Json = serde_json::from_str(raw)
                        .map_err(|e| Error::at(site, format!("invalid tool arguments: {e}")))?;
                    let object = args
                        .as_object()
                        .ok_or_else(|| Error::at(site, "tool arguments must be an object"))?;
                    let t = self.shared.compiled.program.tools[tool].clone();
                    if object
                        .keys()
                        .any(|k| !t.params.iter().any(|p| &p.name == k))
                    {
                        return Err(Error::at(site, "unknown tool argument"));
                    }
                    let mut params = Vec::new();
                    let mut ctx = self.global_context();
                    for p in &t.params {
                        let value = if let Some(v) = object.get(&p.name) {
                            Value::from_json(v.clone())
                        } else if let Some(e) = &p.default {
                            ctx.eval(e)?
                        } else {
                            return Err(Error::at(
                                site,
                                format!("missing tool argument '{}'", p.name),
                            ));
                        };
                        ensure_type(&self.shared.compiled.program, &p.ty, &value, site)?;
                        params.push(value);
                    }
                    planned.push((tool.to_string(), id.to_string(), params));
                }
                assistant["role"] = "assistant".into();
                messages.push(assistant);
                // Each handler owns its own commit; the whole batch is not a transaction.
                for (tool, id, args) in planned {
                    self.tick(site)?;
                    let result = self.tool(&tool, args, site)?;
                    tool_count += 1;
                    used_tools.push(tool);
                    messages.push(json!({"role":"tool","tool_call_id":id,"content":result.to_json(site)?.to_string()}));
                }
                continue;
            }
            let text = assistant_text(&assistant, site)?;
            let data = if let Some(n) = &response {
                let candidate = json_candidate(&text);
                let json: Json = serde_json::from_str(candidate).map_err(|e| {
                    Error::at(site, format!("agent '{name}' returned invalid JSON: {e}"))
                })?;
                let value = Value::from_json(json);
                ensure_type(&self.shared.compiled.program, &Type::named(n), &value, site)?;
                value
            } else {
                Value::String(text.clone())
            };
            operation.success = true;
            let provenance = Value::Object(
                [
                    ("agent".into(), Value::String(name.into())),
                    ("model".into(), Value::String(model.clone())),
                    ("file".into(), Value::String(site.file.clone())),
                    ("line".into(), Value::Int(site.line as i64)),
                    ("column".into(), Value::Int(site.column as i64)),
                    ("offset".into(), Value::Int(site.offset as i64)),
                    ("endLine".into(), Value::Int(site.end_line as i64)),
                    ("endColumn".into(), Value::Int(site.end_column as i64)),
                    ("endOffset".into(), Value::Int(site.end_offset as i64)),
                    (
                        "sourceFingerprint".into(),
                        Value::String(self.shared.compiled.fingerprint()),
                    ),
                    (
                        "tools".into(),
                        Value::Array(used_tools.into_iter().map(Value::String).collect()),
                    ),
                ]
                .into_iter()
                .collect(),
            );
            return Ok(Value::Object(
                [
                    ("text".into(), Value::String(text)),
                    ("data".into(), data),
                    ("model".into(), Value::String(model)),
                    ("toolCalls".into(), Value::Int(tool_count as i64)),
                    ("provenance".into(), provenance),
                ]
                .into_iter()
                .collect(),
            ));
        }
        Err(Error::at(
            site,
            "agent reached maxTurns without a final response",
        ))
    }
}
fn required_text(c: &BTreeMap<String, Value>, key: &str, s: &Site) -> Result<String> {
    c.get(key)
        .ok_or_else(|| Error::at(s, format!("missing '{key}'")))?
        .text(s)
}
fn bounded_int(
    c: &BTreeMap<String, Value>,
    key: &str,
    default: usize,
    min: usize,
    max: usize,
    s: &Site,
) -> Result<usize> {
    let value = match c.get(key) {
        Some(v) => v.integer(s)?,
        None => default as i64,
    };
    if value < min as i64 || value > max as i64 {
        return Err(Error::at(s, format!("{key} must be {min}..{max}")));
    }
    Ok(value as usize)
}
fn flag(c: &BTreeMap<String, Value>, key: &str, s: &Site) -> Result<bool> {
    match c.get(key) {
        Some(v) => v.boolean(s),
        None => Ok(false),
    }
}
fn names(v: Option<&Value>, s: &Site) -> Result<BTreeSet<String>> {
    match v {
        None => Ok(BTreeSet::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| {
                if let Value::Tool(n) = v {
                    Ok(n.clone())
                } else {
                    Err(Error::at(s, "expected declared tool"))
                }
            })
            .collect(),
        _ => Err(Error::at(s, "expected tool list")),
    }
}
fn authorize(
    tool: &str,
    cap: Capability,
    p: &BTreeMap<String, Value>,
    allowed: &BTreeSet<String>,
    s: &Site,
) -> Result<()> {
    if !allowed.contains(tool) {
        return Err(Error::at(s, format!("policy denies tool '{tool}'")));
    }
    let key = match cap {
        Capability::Pure => return Ok(()),
        Capability::FileRead => "allowFileRead",
        Capability::FileWrite => "allowFileWrite",
        Capability::Network => "allowNetwork",
        Capability::Shell => "allowShell",
        Capability::Model | Capability::Microphone => {
            return Err(Error::at(
                s,
                "agent tools cannot declare model or microphone capability",
            ))
        }
    };
    if flag(p, key, s)? {
        Ok(())
    } else {
        Err(Error::at(s, format!("policy denies capability '{key}'")))
    }
}
fn user_message(prompt: Value, s: &Site) -> Result<Json> {
    let content = match prompt {
        Value::String(t) => Json::String(t),
        Value::Object(o) if o.contains_key("audio") => {
            let text = o
                .get("text")
                .ok_or_else(|| Error::at(s, "audio prompt requires text"))?
                .text(s)?;
            let audio = o.get("audio").unwrap();
            let mut parts = vec![json!({"type":"text","text":text})];
            let audios = match audio {
                Value::Array(a) => a.clone(),
                v => vec![v.clone()],
            };
            for value in audios {
                let audio = if let Value::Audio(a) = value {
                    a
                } else {
                    return Err(Error::at(
                        s,
                        "audio prompt requires Audio values, not paths",
                    ));
                };
                parts.push(json!({"type":"input_audio","input_audio":{"format":audio.format,"data":audio.data_base64}}));
            }
            if o.keys().any(|k| k != "audio" && k != "text") {
                return Err(Error::at(s, "audio prompt accepts only text and audio"));
            }
            Json::Array(parts)
        }
        v => Json::String(v.to_json(s)?.to_string()),
    };
    Ok(json!({"role":"user","content":content}))
}
fn assistant_text(v: &Json, s: &Site) -> Result<String> {
    match v.get("content") {
        Some(Json::String(t)) => Ok(t.clone()),
        Some(Json::Array(parts)) => Ok(parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Json::as_str))
            .collect::<Vec<_>>()
            .join("")),
        _ => Err(Error::at(s, "provider response has no text content")),
    }
}
fn json_candidate(text: &str) -> &str {
    let mut t = text.trim();
    if t.starts_with("<think>") {
        if let Some(i) = t.find("</think>") {
            t = t[i + 8..].trim();
        }
    }
    t.strip_prefix("```json")
        .or_else(|| t.strip_prefix("```"))
        .and_then(|t| t.trim().strip_suffix("```"))
        .unwrap_or(t)
        .trim()
}
fn schema(p: &Program, t: &Type, s: &Site, depth: usize) -> Result<Json> {
    if depth > 16 {
        return Err(Error::at(
            s,
            "model schema nesting exceeds 16; recursive schemas unsupported",
        ));
    }
    match t {
        Type::Array(t) => Ok(json!({"type":"array","items":schema(p,t,s,depth+1)?})),
        Type::Task(_) => Err(Error::at(s, "tasks cannot cross a model boundary")),
        Type::Named(n) => match n.as_str() {
            "string" => Ok(json!({"type":"string"})),
            "int" => Ok(json!({"type":"integer"})),
            "double" => Ok(json!({"type":"number"})),
            "bool" => Ok(json!({"type":"boolean"})),
            "any" | "object" => Ok(json!({})),
            _ => {
                let fields = p.records.get(n).ok_or_else(|| {
                    Error::at(s, format!("type '{n}' cannot cross model boundary"))
                })?;
                let props: serde_json::Map<String, Json> = fields
                    .iter()
                    .map(|(k, t)| Ok((k.clone(), schema(p, t, s, depth + 1)?)))
                    .collect::<Result<_>>()?;
                Ok(
                    json!({"type":"object","properties":props,"required":fields.keys().collect::<Vec<_>>(),"additionalProperties":false}),
                )
            }
        },
    }
}
