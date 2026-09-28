//! smllm-json impls writing and reading the serde feature's JSON, byte for
//! byte (PLAN-007), for `smllm-wasm`. Structs go through [`object!`], whose
//! field list mirrors the serde derive: the JSON name, and whether serde
//! requires the field (`req`), defaults it (`default`; every `Option` field
//! is one) or also leaves it out while empty (`skip`). The tests prove the
//! two agree.
// @zen-component: ENG-Json

use smllm_json::{Error, Fields, FromJson, JsonValue, ToJson, object};

use crate::engine::{InstanceStatus, Location, Reply, SessionStatus};
use crate::host::MemoryStore;
use crate::model::{
    ActionDef, Config, EventDef, GuardDef, InstanceSpec, Machine, On, ParamSpec, Position, Prompt,
    SharedAction, State, Transition, Value,
};
use crate::prelude::*;
use crate::record::{HistoryEntry, Instance, InstanceKey, Session, Status};
use crate::utils::SmallMap;

/// `ToJson` + `FromJson` for a struct as a JSON object of the given names.
macro_rules! object {
    ($ty:ident { $($f:ident: $name:literal $mode:ident),* $(,)? }) => {
        impl ToJson for $ty {
            fn write_json(&self, out: &mut String) {
                let mut o = object(out);
                $(object!(@write o, self.$f, $name, $mode);)*
                o.end();
            }
        }

        impl FromJson for $ty {
            fn from_json(value: JsonValue) -> Result<Self, Error> {
                let mut fields = Fields::from_json(value)?;
                Ok($ty { $($f: object!(@read fields, $name, $mode),)* })
            }
        }
    };
    (@write $o:ident, $v:expr, $name:literal, skip) => {
        if !$v.is_empty() {
            $o.field($name, &$v);
        }
    };
    (@write $o:ident, $v:expr, $name:literal, $mode:ident) => {
        $o.field($name, &$v);
    };
    (@read $f:ident, $name:literal, req) => {
        $f.req($name)?
    };
    (@read $f:ident, $name:literal, $mode:ident) => {
        $f.or_default($name)?
    };
}

object!(Config {
    machines: "machines" req,
    idle: "idle" default,
});

object!(Machine {
    id: "id" req,
    description: "description" default,
    initial: "initial" req,
    instance: "instance" req,
    events: "events" req,
    shared: "shared" req,
    states: "states" req,
});

object!(InstanceSpec {
    kind: "kind" req,
    ref_param: "refParam" req,
    ref_description: "refDescription" default,
    ref_pattern: "refPattern" default,
});

object!(EventDef {
    description: "description" default,
    params: "params" req,
});

object!(ParamSpec {
    name: "name" req,
    description: "description" default,
    required: "required" req,
    enum_values: "enumValues" req,
    pattern: "pattern" default,
});

object!(SharedAction {
    states: "states" req,
    position: "position" req,
    entry: "entry" req,
    exit: "exit" req,
});

object!(State {
    name: "name" req,
    description: "description" default,
    is_final: "isFinal" req,
    entry_point: "entryPoint" req,
    fallback: "fallback" req,
    entry: "entry" req,
    exit: "exit" req,
    on: "on" req,
    always: "always" req,
    param_descriptions: "paramDescriptions" req,
});

object!(On {
    event: "event" req,
    transitions: "transitions" req,
});

object!(Transition {
    target: "target" default,
    guard: "guard" default,
    actions: "actions" req,
    reenter: "reenter" req,
    description: "description" default,
});

object!(InstanceKey {
    machine: "machine" req,
    id: "id" req,
});

object!(Instance {
    id: "id" req,
    machine: "machine" req,
    r#ref: "ref" default,
    state: "state" req,
    status: "status" req,
    holder: "holder" default,
    version: "version" req,
    visits: "visits" default,
    resume_state: "resumeState" default,
    created: "created" req,
    updated: "updated" req,
});

object!(Session {
    key: "key" req,
    harness: "harness" req,
    host_session: "hostSession" default,
    cwd: "cwd" req,
    configs: "configs" default,
    holding: "holding" default,
    interrupted: "interrupted" default,
    yielded: "yielded" default,
    blocked: "blocked" default,
    created: "created" req,
    last_active: "lastActive" req,
});

object!(HistoryEntry {
    at: "at" req,
    session: "session" req,
    event: "event" req,
    from: "from" default,
    to: "to" default,
    params: "params" default,
    trace: "trace" default,
});

object!(MemoryStore {
    sessions: "sessions" req,
    bindings: "bindings" req,
    instances: "instances" req,
    history: "history" skip,
});

object!(Location {
    machine: "machine" default,
    state: "state" default,
    instance: "instance" default,
    r#ref: "ref" default,
});

object!(Reply {
    ok: "ok" req,
    session: "session" req,
    location: "location" req,
    text: "text" req,
});

object!(SessionStatus {
    session: "session" req,
    idle: "idle" req,
    machine: "machine" default,
    state: "state" default,
    visit: "visit" default,
    yielded: "yielded" req,
    instance: "instance" default,
    interrupted: "interrupted" default,
    paused: "paused" req,
});

object!(InstanceStatus {
    machine: "machine" req,
    kind: "kind" req,
    id: "id" req,
    r#ref: "ref" default,
    label: "label" req,
    status: "status" req,
});

impl<V: ToJson> ToJson for SmallMap<V> {
    fn write_json(&self, out: &mut String) {
        let mut o = object(out);
        for (k, v) in self.iter() {
            o.field(k, v);
        }
        o.end();
    }
}

/// Key order, a repeated key's last value winning (as serde reads a map).
impl<V: FromJson> FromJson for SmallMap<V> {
    fn from_json(value: JsonValue) -> Result<Self, Error> {
        let mut map = SmallMap::new();
        for (k, v) in Fields::from_json(value)?.into_vec() {
            let v = V::from_json(v).map_err(|e| e.in_field(&k))?;
            map.insert(k, v);
        }
        Ok(map)
    }
}

/// A unit variant: its name as a string.
fn unit<T: Copy>(value: JsonValue, names: &[(&str, T)]) -> Result<T, Error> {
    let name = String::from_json(value)?;
    names
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| *v)
        .ok_or_else(|| Error::new(format!("unknown variant `{name}`")))
}

const POSITIONS: [(&str, Position); 2] = [("before", Position::Before), ("after", Position::After)];

impl ToJson for Position {
    fn write_json(&self, out: &mut String) {
        let name = POSITIONS
            .iter()
            .find(|(_, p)| p == self)
            .map_or("", |(n, _)| n);
        name.write_json(out);
    }
}

impl FromJson for Position {
    fn from_json(value: JsonValue) -> Result<Self, Error> {
        unit(value, &POSITIONS)
    }
}

impl ToJson for Status {
    fn write_json(&self, out: &mut String) {
        self.as_str().write_json(out);
    }
}

impl FromJson for Status {
    fn from_json(value: JsonValue) -> Result<Self, Error> {
        let all = [
            Status::Active,
            Status::Interrupted,
            Status::Paused,
            Status::Completed,
        ];
        unit(value, &all.map(|s| (s.as_str(), s)))
    }
}

/// Untagged: a string, integer, boolean or list of strings.
impl ToJson for Value {
    fn write_json(&self, out: &mut String) {
        match self {
            Value::Str(s) => s.write_json(out),
            Value::Int(n) => n.write_json(out),
            Value::Bool(b) => b.write_json(out),
            Value::List(l) => l.write_json(out),
        }
    }
}

impl FromJson for Value {
    fn from_json(value: JsonValue) -> Result<Self, Error> {
        match value {
            JsonValue::String(s) => Ok(Value::Str(s)),
            JsonValue::Int(n) => Ok(Value::Int(n)),
            JsonValue::Bool(b) => Ok(Value::Bool(b)),
            v @ JsonValue::Array(_) => Vec::from_json(v).map(Value::List),
            other => Err(Error::new(format!(
                "expected a string, integer, boolean or list of strings, found {}",
                other.kind()
            ))),
        }
    }
}

/// An externally tagged variant, `{"tag": value}`.
fn tagged(out: &mut String, tag: &str, value: &dyn ToJson) {
    let mut o = object(out);
    o.field(tag, value);
    o.end();
}

/// A struct variant's fields (serde renames the variant, not its fields).
struct Visits<'a>(&'a String, &'a u32);

impl ToJson for Visits<'_> {
    fn write_json(&self, out: &mut String) {
        let mut o = object(out);
        o.field("state", self.0).field("at_least", self.1);
        o.end();
    }
}

struct Host<'a>(&'a String, &'a SmallMap<Value>);

impl ToJson for Host<'_> {
    fn write_json(&self, out: &mut String) {
        let mut o = object(out);
        o.field("kind", self.0).field("params", self.1);
        o.end();
    }
}

fn host(value: JsonValue) -> Result<(String, SmallMap<Value>), Error> {
    let mut f = Fields::from_json(value)?;
    Ok((f.req("kind")?, f.req("params")?))
}

impl ToJson for GuardDef {
    fn write_json(&self, out: &mut String) {
        match self {
            GuardDef::Visits { state, at_least } => tagged(out, "visits", &Visits(state, at_least)),
            GuardDef::Host { kind, params } => tagged(out, "host", &Host(kind, params)),
        }
    }
}

impl FromJson for GuardDef {
    fn from_json(value: JsonValue) -> Result<Self, Error> {
        let (tag, body) = Fields::from_json(value)?.into_single()?;
        match tag.as_str() {
            "visits" => {
                let mut f = Fields::from_json(body).map_err(|e| e.in_field("visits"))?;
                Ok(GuardDef::Visits {
                    state: f.req("state").map_err(|e| e.in_field("visits"))?,
                    at_least: f.req("at_least").map_err(|e| e.in_field("visits"))?,
                })
            }
            "host" => {
                let (kind, params) = host(body).map_err(|e| e.in_field("host"))?;
                Ok(GuardDef::Host { kind, params })
            }
            _ => Err(Error::new(format!("unknown variant `{tag}`"))),
        }
    }
}

impl ToJson for Prompt {
    fn write_json(&self, out: &mut String) {
        match self {
            Prompt::Text(s) => tagged(out, "text", s),
            Prompt::File(s) => tagged(out, "file", s),
            Prompt::DefaultFile(s) => tagged(out, "defaultFile", s),
        }
    }
}

impl FromJson for Prompt {
    fn from_json(value: JsonValue) -> Result<Self, Error> {
        let (tag, body) = Fields::from_json(value)?.into_single()?;
        let make = match tag.as_str() {
            "text" => Prompt::Text,
            "file" => Prompt::File,
            "defaultFile" => Prompt::DefaultFile,
            _ => return Err(Error::new(format!("unknown variant `{tag}`"))),
        };
        String::from_json(body)
            .map(make)
            .map_err(|e| e.in_field(&tag))
    }
}

impl ToJson for ActionDef {
    fn write_json(&self, out: &mut String) {
        match self {
            ActionDef::Prompt(p) => tagged(out, "prompt", p),
            ActionDef::SetRef => "setRef".write_json(out),
            ActionDef::Host { kind, params } => tagged(out, "host", &Host(kind, params)),
        }
    }
}

impl FromJson for ActionDef {
    fn from_json(value: JsonValue) -> Result<Self, Error> {
        if let JsonValue::String(s) = &value {
            return match s.as_str() {
                "setRef" => Ok(ActionDef::SetRef),
                _ => Err(Error::new(format!("unknown variant `{s}`"))),
            };
        }
        let (tag, body) = Fields::from_json(value)?.into_single()?;
        match tag.as_str() {
            "prompt" => Prompt::from_json(body)
                .map(ActionDef::Prompt)
                .map_err(|e| e.in_field("prompt")),
            "host" => {
                let (kind, params) = host(body).map_err(|e| e.in_field("host"))?;
                Ok(ActionDef::Host { kind, params })
            }
            _ => Err(Error::new(format!("unknown variant `{tag}`"))),
        }
    }
}

#[cfg(all(test, feature = "serde"))]
mod tests;
