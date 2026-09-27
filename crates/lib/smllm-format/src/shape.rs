//! Structural pre-pass: walk the YAML as a generic tree against the format's
//! shape and report EVERY unknown key, wrong type and unsupported XState
//! feature, each with its YAML path and line (CFG-2, CFG-14). serde stops at
//! the first problem; this pass does not. A key whose value is null (`key:`
//! or `key: ~`) counts as absent, as in the JSON Schema; [`normalise`]
//! removes it before serde reads the document.
// @zen-component: CFG-Source

use std::sync::OnceLock;

use schemars::JsonSchema;
use serde_json::{Map, Value};

use crate::lower::Checker;
use crate::source::{
    CommandParams, EventMeta, InstanceMeta, MachineFile, MachineMeta, ParamsSchema, PromptParams,
    PropSchema, RefMeta, SharedActionSrc, StateMeta, StateNode, TransitionSrc, VisitsParams,
};

/// A source type's keys, read from its JSON Schema: the one the serde types
/// derive, so this walk and the parser can never disagree on a key.
struct Keys {
    allowed: Vec<String>,
    required: Vec<String>,
}

impl Keys {
    /// From `T`'s definition in the machine file's schema, built once; a
    /// type the schema inlines gets its own.
    fn of<T: JsonSchema>() -> Self {
        static ROOT: OnceLock<schemars::Schema> = OnceLock::new();
        let root = ROOT.get_or_init(|| schemars::schema_for!(MachineFile));
        let own;
        let v = match root.get("$defs").and_then(|d| d.get(T::schema_name().as_ref())) {
            Some(v) => v,
            None if T::schema_name() == MachineFile::schema_name() => root.as_value(),
            None => {
                own = schemars::schema_for!(T);
                own.as_value()
            }
        };
        let names = |key: &str| -> Vec<String> {
            match v.get(key) {
                Some(Value::Object(m)) => m.keys().cloned().collect(),
                Some(Value::Array(a)) => a
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
                _ => Vec::new(),
            }
        };
        Keys {
            allowed: names("properties"),
            required: names("required"),
        }
    }
}

/// The [`Keys`] of a source type, computed once.
macro_rules! keys {
    ($t:ty) => {{
        static KEYS: OnceLock<Keys> = OnceLock::new();
        KEYS.get_or_init(Keys::of::<$t>)
    }};
}

/// XState keys smllm v1 does not support: an error with a v1 hint.
// @zen-impl: CFG-2_AC-1
const UNSUPPORTED: [&str; 11] = [
    "states", "initial", "invoke", "after", "context", "output", "tags", "history", "onDone",
    "types", "version",
];

type Path = Vec<String>;

fn at(path: &[String], seg: impl Into<String>) -> Path {
    let mut p = path.to_vec();
    p.push(seg.into());
    p
}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "nothing",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "a list",
        Value::Object(_) => "a map",
    }
}

struct Walk<'c, 'a> {
    c: &'c mut Checker<'a>,
}

impl Walk<'_, '_> {
    fn err(&mut self, path: &[String], msg: String, hint: Option<&str>, rule: &'static str) {
        self.c.error(path, msg, hint, rule);
    }

    fn wrong(&mut self, path: &[String], want: &str, got: &Value) {
        self.err(
            path,
            format!("expected {want}, found {}", kind(got)),
            None,
            "CFG-1",
        );
    }

    /// A map with `allowed` keys; `required` must be present. Returns it for
    /// the caller to walk its values.
    fn map<'v, S: AsRef<str>>(
        &mut self,
        path: &[String],
        v: &'v Value,
        allowed: &[S],
        required: &[S],
    ) -> Option<&'v Map<String, Value>> {
        let Value::Object(m) = v else {
            self.wrong(path, "a map", v);
            return None;
        };
        for k in m.keys() {
            if allowed.iter().any(|a| a.as_ref() == k) {
                continue;
            }
            if UNSUPPORTED.contains(&k.as_str()) {
                self.err(
                    &at(path, k),
                    format!("`{k}` is not supported"),
                    Some("XState, but not in smllm v1's subset: flat atomic/final states; no delays, invocations, context"),
                    "CFG-2",
                );
            } else {
                self.err(
                    &at(path, k),
                    format!(
                        "unknown key `{k}` (expected one of: {})",
                        allowed
                            .iter()
                            .map(AsRef::as_ref)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    None,
                    "CFG-1",
                );
            }
        }
        for r in required {
            let r = r.as_ref();
            if m.get(r).is_none_or(Value::is_null) {
                self.err(path, format!("missing key `{r}`"), None, "CFG-1");
            }
        }
        Some(m)
    }

    fn string(&mut self, path: &[String], v: Option<&Value>) {
        if let Some(v) = present(v)
            && !v.is_string()
        {
            self.wrong(path, "a string", v);
        }
    }

    fn strings(&mut self, path: &[String], v: Option<&Value>) {
        match present(v) {
            None => {}
            Some(Value::Array(items)) => {
                for (i, it) in items.iter().enumerate() {
                    self.string(&at(path, format!("[{i}]")), Some(it));
                }
            }
            Some(v) => self.wrong(path, "a list of strings", v),
        }
    }

    fn boolean(&mut self, path: &[String], v: Option<&Value>) {
        if let Some(v) = present(v)
            && !v.is_boolean()
        {
            self.wrong(path, "true or false", v);
        }
    }

    fn uint(&mut self, path: &[String], v: Option<&Value>) {
        if let Some(v) = present(v)
            && !v.is_u64()
        {
            self.wrong(path, "a whole number", v);
        }
    }

    fn one_of(&mut self, path: &[String], v: Option<&Value>, values: &[&str], rule: &'static str) {
        match present(v) {
            None => {}
            Some(Value::String(s)) if values.contains(&s.as_str()) => {}
            Some(Value::String(s)) => self.err(
                path,
                format!("`{s}` is not one of: {}", values.join(", ")),
                None,
                rule,
            ),
            Some(v) => self.wrong(path, "a string", v),
        }
    }

    fn each<'v>(&mut self, path: &[String], v: Option<&'v Value>) -> Vec<(Path, &'v Value)> {
        match present(v) {
            None => Vec::new(),
            Some(Value::Object(m)) => m.iter().map(|(k, v)| (at(path, k), v)).collect(),
            Some(v) => {
                self.wrong(path, "a map", v);
                Vec::new()
            }
        }
    }

    fn machine(&mut self, v: &Value) {
        let root = Vec::new();
        let k = keys!(MachineFile);
        let Some(m) = self.map(&root, v, &k.allowed, &k.required) else {
            return;
        };
        for k in ["id", "description", "initial"] {
            self.string(&at(&root, k), m.get(k));
        }
        if let Some(meta) = present(m.get("meta")) {
            self.meta(meta);
        }
        for (p, s) in self.each(&at(&root, "states"), m.get("states")) {
            self.state(&p, s);
        }
    }

    fn meta(&mut self, v: &Value) {
        let p = vec!["meta".to_string()];
        let k = keys!(MachineMeta);
        let Some(m) = self.map(&p, v, &k.allowed, &k.required) else {
            return;
        };
        self.uint(&at(&p, "smllm"), m.get("smllm"));
        if let Some(i) = present(m.get("instance")) {
            let ip = at(&p, "instance");
            let k = keys!(InstanceMeta);
            if let Some(im) = self.map(&ip, i, &k.allowed, &k.required) {
                self.string(&at(&ip, "kind"), im.get("kind"));
                if let Some(r) = present(im.get("ref")) {
                    let rp = at(&ip, "ref");
                    let k = keys!(RefMeta);
                    if let Some(rm) = self.map(&rp, r, &k.allowed, &k.required) {
                        for k in ["param", "description", "pattern"] {
                            self.string(&at(&rp, k), rm.get(k));
                        }
                    }
                }
            }
        }
        for (ep, e) in self.each(&at(&p, "events"), m.get("events")) {
            let k = keys!(EventMeta);
            if let Some(em) = self.map(&ep, e, &k.allowed, &k.required) {
                self.string(&at(&ep, "description"), em.get("description"));
                if let Some(ps) = present(em.get("params")) {
                    self.params(&at(&ep, "params"), ps);
                }
            }
        }
        match present(m.get("sharedActions")) {
            None => {}
            Some(Value::Array(items)) => {
                for (i, it) in items.iter().enumerate() {
                    let sp = at(&at(&p, "sharedActions"), format!("[{i}]"));
                    let k = keys!(SharedActionSrc);
                    if let Some(sm) = self.map(&sp, it, &k.allowed, &k.required) {
                        self.strings(&at(&sp, "states"), sm.get("states"));
                        self.one_of(
                            &at(&sp, "position"),
                            sm.get("position"),
                            &["before", "after"],
                            "CFG-11",
                        );
                        for k in ["entry", "exit"] {
                            self.actions(&at(&sp, k), sm.get(k));
                        }
                    }
                }
            }
            Some(v) => self.wrong(&at(&p, "sharedActions"), "a list", v),
        }
    }

    fn params(&mut self, p: &[String], v: &Value) {
        let k = keys!(ParamsSchema);
        let Some(m) = self.map(p, v, &k.allowed, &k.required) else {
            return;
        };
        self.string(&at(p, "type"), m.get("type"));
        self.strings(&at(p, "required"), m.get("required"));
        for (pp, prop) in self.each(&at(p, "properties"), m.get("properties")) {
            let k = keys!(PropSchema);
            if let Some(pm) = self.map(&pp, prop, &k.allowed, &k.required) {
                for k in ["type", "description", "pattern"] {
                    self.string(&at(&pp, k), pm.get(k));
                }
                self.strings(&at(&pp, "enum"), pm.get("enum"));
            }
        }
    }

    fn state(&mut self, p: &[String], v: &Value) {
        let k = keys!(StateNode);
        let Some(m) = self.map(p, v, &k.allowed, &k.required) else {
            return;
        };
        self.string(&at(p, "description"), m.get("description"));
        match m.get("type") {
            Some(Value::String(t)) if matches!(t.as_str(), "parallel" | "history" | "compound") => {
                self.err(
                    &at(p, "type"),
                    format!("`type: {t}` is not supported"),
                    Some("XState, but not in smllm v1's subset: flat atomic/final states"),
                    "CFG-2",
                )
            }
            other => self.one_of(&at(p, "type"), other, &["atomic", "final"], "CFG-12"),
        }
        if let Some(meta) = present(m.get("meta")) {
            let mp = at(p, "meta");
            let k = keys!(StateMeta);
            if let Some(mm) = self.map(&mp, meta, &k.allowed, &k.required) {
                self.boolean(&at(&mp, "entryPoint"), mm.get("entryPoint"));
                self.boolean(&at(&mp, "fallback"), mm.get("fallback"));
                for (ep, prompts) in
                    self.each(&at(&mp, "paramDescriptions"), mm.get("paramDescriptions"))
                {
                    for (pp, text) in self.each(&ep, Some(prompts)) {
                        self.string(&pp, Some(text));
                    }
                }
            }
        }
        for k in ["entry", "exit"] {
            self.actions(&at(p, k), m.get(k));
        }
        for (ep, ts) in self.each(&at(p, "on"), m.get("on")) {
            self.transitions(&ep, ts);
        }
        if let Some(ts) = present(m.get("always")) {
            self.transitions(&at(p, "always"), ts);
        }
    }

    fn transitions(&mut self, p: &[String], v: &Value) {
        match v {
            Value::Array(items) if items.len() == 1 => self.transition(p, &items[0]),
            Value::Array(items) => {
                for (i, it) in items.iter().enumerate() {
                    self.transition(&at(p, format!("[{i}]")), it);
                }
            }
            v => self.transition(p, v),
        }
    }

    fn transition(&mut self, p: &[String], v: &Value) {
        if v.is_string() {
            return;
        }
        let k = keys!(TransitionSrc);
        let Some(m) = self.map(p, v, &k.allowed, &k.required) else {
            return;
        };
        self.string(&at(p, "target"), m.get("target"));
        self.string(&at(p, "description"), m.get("description"));
        self.boolean(&at(p, "reenter"), m.get("reenter"));
        self.actions(&at(p, "actions"), m.get("actions"));
        if let Some(g) = present(m.get("guard")) {
            self.guard(&at(p, "guard"), g);
        }
    }

    fn actions(&mut self, p: &[String], v: Option<&Value>) {
        match present(v) {
            None => {}
            Some(Value::Array(items)) if items.len() == 1 => self.action(p, &items[0]),
            Some(Value::Array(items)) => {
                for (i, it) in items.iter().enumerate() {
                    self.action(&at(p, format!("[{i}]")), it);
                }
            }
            Some(v) => self.action(p, v),
        }
    }

    fn action(&mut self, p: &[String], v: &Value) {
        if let Value::String(s) = v {
            if s != "setRef" {
                self.err(
                    p,
                    format!("unknown action `{s}`"),
                    Some("write actions as {type, params}; the only param-less action is setRef"),
                    "CFG-4",
                );
            }
            return;
        }
        let Some(m) = self.map(p, v, &["type", "params"], &["type"]) else {
            return;
        };
        let params = present(m.get("params"));
        let pp = at(p, "params");
        match m.get("type").and_then(Value::as_str) {
            Some("prompt") => {
                let k = keys!(PromptParams);
                self.keys(&pp, params, &k.allowed, &k.required, |w, k, v| {
                    w.string(k, Some(v))
                });
                let given = ["text", "file"]
                    .iter()
                    .filter(|k| present(params.and_then(|m| m.get(**k))).is_some())
                    .count();
                if given != 1 && params.is_none_or(Value::is_object) {
                    self.err(
                        &pp,
                        "a prompt needs exactly one of: text, file".to_string(),
                        None,
                        "CFG-4",
                    );
                }
            }
            Some("command") => self.command(&pp, params),
            Some("setRef") => self.keys::<&str>(&pp, params, &[], &[], |_, _, _| {}),
            Some(t) => self.err(
                &at(p, "type"),
                format!("unknown action type `{t}` (expected one of: prompt, command, setRef)"),
                None,
                "CFG-4",
            ),
            None => self.wrong(
                &at(p, "type"),
                "a string",
                m.get("type").unwrap_or(&Value::Null),
            ),
        }
    }

    fn guard(&mut self, p: &[String], v: &Value) {
        if v.is_string() {
            self.err(
                p,
                "a guard is written as {type, params}".to_string(),
                Some("guard types: command { run }, visits { state, atLeast }"),
                "CFG-5",
            );
            return;
        }
        let Some(m) = self.map(p, v, &["type", "params"], &["type", "params"]) else {
            return;
        };
        let params = present(m.get("params"));
        let pp = at(p, "params");
        match m.get("type").and_then(Value::as_str) {
            Some("command") => self.command(&pp, params),
            Some("visits") => self.keys(
                &pp,
                params,
                &keys!(VisitsParams).allowed,
                &keys!(VisitsParams).required,
                |w, k, v| {
                    if k.last().is_some_and(|l| l == "atLeast") {
                        w.uint(k, Some(v));
                    } else {
                        w.string(k, Some(v));
                    }
                },
            ),
            Some(t) => self.err(
                &at(p, "type"),
                format!("unknown guard type `{t}` (expected one of: command, visits)"),
                None,
                "CFG-5",
            ),
            None => self.wrong(
                &at(p, "type"),
                "a string",
                m.get("type").unwrap_or(&Value::Null),
            ),
        }
    }

    fn command(&mut self, p: &[String], params: Option<&Value>) {
        let k = keys!(CommandParams);
        self.keys(p, params, &k.allowed, &k.required, |w, k, v| {
            match k.last().map(String::as_str) {
                Some("run") => match v {
                    Value::String(_) => {}
                    Value::Array(items) if items.iter().all(Value::is_string) => {}
                    _ => w.err(
                        k,
                        "run must be a string (shell) or a list of strings (no shell)".to_string(),
                        None,
                        "DEC-4",
                    ),
                },
                Some("timeoutSecs") => w.uint(k, Some(v)),
                _ => w.string(k, Some(v)),
            }
        });
    }

    /// `params`: a map with `allowed` keys (absent = empty when nothing is
    /// required), each value checked by `check`.
    fn keys<S: AsRef<str>>(
        &mut self,
        p: &[String],
        params: Option<&Value>,
        allowed: &[S],
        required: &[S],
        check: impl Fn(&mut Self, &[String], &Value),
    ) {
        let empty = Value::Object(Map::new());
        let Some(m) = self.map(p, params.unwrap_or(&empty), allowed, required) else {
            return;
        };
        for (k, v) in m {
            if allowed.iter().any(|a| a.as_ref() == k) && !v.is_null() {
                check(self, &at(p, k), v);
            }
        }
    }
}

/// `v`, unless it is absent or null (the same to the format).
fn present(v: Option<&Value>) -> Option<&Value> {
    v.filter(|v| !v.is_null())
}

/// Check `doc`'s shape; findings go to `c`.
pub(crate) fn check(c: &mut Checker<'_>, doc: &Value) {
    Walk { c }.machine(doc);
}

/// Null values → absent keys, and `{type: setRef, params: {}}` →
/// `{type: setRef}` (serde's unit variant takes no content).
pub(crate) fn normalise(doc: &mut Value) {
    match doc {
        Value::Object(m) => {
            m.retain(|_, v| !v.is_null());
            if m.get("type").and_then(Value::as_str) == Some("setRef")
                && m.get("params")
                    .is_some_and(|p| p.as_object().is_some_and(Map::is_empty))
            {
                m.remove("params");
            }
            for v in m.values_mut() {
                normalise(v);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(normalise),
        _ => {}
    }
}
