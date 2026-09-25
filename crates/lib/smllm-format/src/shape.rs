//! Structural pre-pass: walk the YAML as a generic tree against the format's
//! shape and report EVERY unknown key, wrong type and unsupported XState
//! feature, each with its YAML path and line (CFG-2, CFG-14). serde stops at
//! the first problem; this pass does not.
// @zen-component: CFG-Source

use serde_json::{Map, Value};

use crate::lower::Checker;

/// XState keys smllm v1 does not support, with the hint's feature name.
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
    fn map<'v>(
        &mut self,
        path: &[String],
        v: &'v Value,
        allowed: &[&str],
        required: &[&str],
    ) -> Option<&'v Map<String, Value>> {
        let Value::Object(m) = v else {
            self.wrong(path, "a map", v);
            return None;
        };
        for k in m.keys() {
            if allowed.contains(&k.as_str()) {
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
                        allowed.join(", ")
                    ),
                    None,
                    "CFG-1",
                );
            }
        }
        for r in required {
            if !m.contains_key(*r) {
                self.err(path, format!("missing key `{r}`"), None, "CFG-1");
            }
        }
        Some(m)
    }

    fn string(&mut self, path: &[String], v: Option<&Value>) {
        if let Some(v) = v
            && !v.is_string()
        {
            self.wrong(path, "a string", v);
        }
    }

    fn strings(&mut self, path: &[String], v: Option<&Value>) {
        match v {
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
        if let Some(v) = v
            && !v.is_boolean()
        {
            self.wrong(path, "true or false", v);
        }
    }

    fn uint(&mut self, path: &[String], v: Option<&Value>) {
        if let Some(v) = v
            && !v.is_u64()
        {
            self.wrong(path, "a whole number", v);
        }
    }

    fn one_of(&mut self, path: &[String], v: Option<&Value>, values: &[&str], rule: &'static str) {
        match v {
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
        match v {
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
        let Some(m) = self.map(
            &root,
            v,
            &["id", "description", "initial", "meta", "states"],
            &["id", "initial", "meta", "states"],
        ) else {
            return;
        };
        for k in ["id", "description", "initial"] {
            self.string(&at(&root, k), m.get(k));
        }
        if let Some(meta) = m.get("meta") {
            self.meta(meta);
        }
        for (p, s) in self.each(&at(&root, "states"), m.get("states")) {
            self.state(&p, s);
        }
    }

    fn meta(&mut self, v: &Value) {
        let p = vec!["meta".to_string()];
        let Some(m) = self.map(
            &p,
            v,
            &["smllm", "instance", "events", "sharedActions"],
            &["smllm"],
        ) else {
            return;
        };
        self.uint(&at(&p, "smllm"), m.get("smllm"));
        if let Some(i) = m.get("instance") {
            let ip = at(&p, "instance");
            if let Some(im) = self.map(&ip, i, &["noun", "ref"], &[]) {
                self.string(&at(&ip, "noun"), im.get("noun"));
                if let Some(r) = im.get("ref") {
                    let rp = at(&ip, "ref");
                    if let Some(rm) = self.map(&rp, r, &["param", "description", "pattern"], &[]) {
                        for k in ["param", "description", "pattern"] {
                            self.string(&at(&rp, k), rm.get(k));
                        }
                    }
                }
            }
        }
        for (ep, e) in self.each(&at(&p, "events"), m.get("events")) {
            if let Some(em) = self.map(&ep, e, &["description", "params"], &[]) {
                self.string(&at(&ep, "description"), em.get("description"));
                if let Some(ps) = em.get("params") {
                    self.params(&at(&ep, "params"), ps);
                }
            }
        }
        match m.get("sharedActions") {
            None => {}
            Some(Value::Array(items)) => {
                for (i, it) in items.iter().enumerate() {
                    let sp = at(&at(&p, "sharedActions"), format!("[{i}]"));
                    if let Some(sm) = self.map(
                        &sp,
                        it,
                        &["states", "position", "entry", "exit"],
                        &["states", "position"],
                    ) {
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
        let Some(m) = self.map(p, v, &["type", "properties", "required"], &["type"]) else {
            return;
        };
        self.string(&at(p, "type"), m.get("type"));
        self.strings(&at(p, "required"), m.get("required"));
        for (pp, prop) in self.each(&at(p, "properties"), m.get("properties")) {
            if let Some(pm) = self.map(
                &pp,
                prop,
                &["type", "description", "enum", "pattern"],
                &["type"],
            ) {
                for k in ["type", "description", "pattern"] {
                    self.string(&at(&pp, k), pm.get(k));
                }
                self.strings(&at(&pp, "enum"), pm.get("enum"));
            }
        }
    }

    fn state(&mut self, p: &[String], v: &Value) {
        let keys = [
            "description",
            "type",
            "meta",
            "entry",
            "exit",
            "on",
            "always",
        ];
        let Some(m) = self.map(p, v, &keys, &[]) else {
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
        if let Some(meta) = m.get("meta") {
            let mp = at(p, "meta");
            if let Some(mm) = self.map(
                &mp,
                meta,
                &["entryPoint", "fallback", "paramDescriptions"],
                &[],
            ) {
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
        if let Some(ts) = m.get("always") {
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
        let Some(m) = self.map(
            p,
            v,
            &["target", "guard", "actions", "reenter", "description"],
            &[],
        ) else {
            return;
        };
        self.string(&at(p, "target"), m.get("target"));
        self.string(&at(p, "description"), m.get("description"));
        self.boolean(&at(p, "reenter"), m.get("reenter"));
        self.actions(&at(p, "actions"), m.get("actions"));
        if let Some(g) = m.get("guard") {
            self.guard(&at(p, "guard"), g);
        }
    }

    fn actions(&mut self, p: &[String], v: Option<&Value>) {
        match v {
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
        let params = m.get("params");
        let pp = at(p, "params");
        match m.get("type").and_then(Value::as_str) {
            Some("prompt") => self.keys(&pp, params, &["text", "file"], &[], |w, k, v| {
                w.string(k, Some(v))
            }),
            Some("command") => self.command(&pp, params),
            Some("setRef") => self.keys(&pp, params, &[], &[], |_, _, _| {}),
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
        let params = m.get("params");
        let pp = at(p, "params");
        match m.get("type").and_then(Value::as_str) {
            Some("command") => self.command(&pp, params),
            Some("visits") => self.keys(
                &pp,
                params,
                &["state", "atLeast"],
                &["state", "atLeast"],
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
        self.keys(
            p,
            params,
            &["run", "timeoutSecs", "cwd"],
            &["run"],
            |w, k, v| match k.last().map(String::as_str) {
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
            },
        );
    }

    /// `params`: a map with `allowed` keys (absent = empty when nothing is
    /// required), each value checked by `check`.
    fn keys(
        &mut self,
        p: &[String],
        params: Option<&Value>,
        allowed: &[&str],
        required: &[&str],
        check: impl Fn(&mut Self, &[String], &Value),
    ) {
        let empty = Value::Object(Map::new());
        let Some(m) = self.map(p, params.unwrap_or(&empty), allowed, required) else {
            return;
        };
        for (k, v) in m {
            if allowed.contains(&k.as_str()) {
                check(self, &at(p, k), v);
            }
        }
    }
}

/// Check `doc`'s shape; findings go to `c`.
pub(crate) fn check(c: &mut Checker<'_>, doc: &Value) {
    Walk { c }.machine(doc);
}

/// `{type: setRef, params: {}}` → `{type: setRef}`: serde's unit variant
/// takes no content.
pub(crate) fn normalise(doc: &mut Value) {
    match doc {
        Value::Object(m) => {
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
