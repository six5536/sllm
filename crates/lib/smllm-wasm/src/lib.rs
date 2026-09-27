//! smllm-wasm: `smllm-core` for JavaScript hosts (browser or Node).
//!
//! The machines come in as `smllm compile` JSON. The JS host supplies guards,
//! actions, prompt files, pattern matching, time and randomness through one
//! object; sessions and instances live in an in-memory store the host can
//! export and import as JSON. Every call returns the core `Reply` as JSON.
// @zen-component: HOST-Wasm

use core::sync::atomic::{AtomicU32, Ordering};
use smllm_core::host::{
    Action, Call, Clock, Guard, Host, Ids, InstructionSource, Matcher, MemoryStore, Outcome,
};
use smllm_core::model::Config;
use smllm_core::{Bind, Stop};

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// The JS host object. Every method must exist. Each is called with
    /// `catch`: a method that throws is a host failure the engine handles
    /// (a failed guard or action, a bad pattern), never an unwind through it.
    pub type JsHost;

    /// Guard of a host kind: `true` passes.
    #[wasm_bindgen(method, catch)]
    fn check(
        this: &JsHost,
        kind: &str,
        params: &str,
        env: &str,
        cwd: &str,
    ) -> Result<bool, JsValue>;
    /// Action of a host kind: `""` = success, else the failure detail.
    #[wasm_bindgen(method, catch)]
    fn run(
        this: &JsHost,
        kind: &str,
        params: &str,
        env: &str,
        cwd: &str,
    ) -> Result<String, JsValue>;
    /// Host kinds supported (`"command"` …).
    #[wasm_bindgen(method, catch)]
    fn supports(this: &JsHost, kind: &str) -> Result<bool, JsValue>;
    /// A prompt file's text, or `undefined` when absent.
    #[wasm_bindgen(method, catch)]
    fn read(this: &JsHost, file: &str) -> Result<Option<String>, JsValue>;
    /// ECMA-262 `new RegExp(pattern).test(value)`.
    #[wasm_bindgen(method, catch, js_name = isMatch)]
    fn is_match(this: &JsHost, pattern: &str, value: &str) -> Result<bool, JsValue>;
    /// `Date.now()`.
    #[wasm_bindgen(method, catch)]
    fn now(this: &JsHost) -> Result<f64, JsValue>;
    /// A random 32-bit unsigned integer.
    #[wasm_bindgen(method, catch)]
    fn random(this: &JsHost) -> Result<u32, JsValue>;

    /// The global `String(value)`: an exception's text (`Error: …`).
    #[wasm_bindgen(js_name = String)]
    fn js_string(value: &JsValue) -> String;
}

/// What a host method threw, as text.
fn thrown(what: &str, e: &JsValue) -> String {
    format!("host {what} threw: {}", js_string(e))
}

/// Ids when the host's `random` throws: distinct per call, so id retries
/// always end.
static FALLBACK_IDS: AtomicU32 = AtomicU32::new(0);

struct Js<'a>(&'a JsHost);

fn json<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

fn env_json(env: &[(String, String)]) -> String {
    let map: serde_json::Map<String, serde_json::Value> = env
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect();
    json(&map)
}

impl Js<'_> {
    /// `supports`, a throw meaning no.
    fn kind_supported(&self, kind: &str) -> bool {
        self.0.supports(kind).unwrap_or(false)
    }

    fn random_u32(&self) -> u32 {
        self.0
            .random()
            .unwrap_or_else(|_| FALLBACK_IDS.fetch_add(1, Ordering::Relaxed))
    }
}

impl Guard for Js<'_> {
    fn supports(&self, kind: &str) -> bool {
        self.kind_supported(kind)
    }
    fn check(&mut self, c: &Call<'_>) -> Outcome {
        match self
            .0
            .check(c.kind, &json(c.params), &env_json(c.env), c.cwd)
        {
            Ok(ok) => Outcome {
                ok,
                detail: String::new(),
            },
            Err(e) => Outcome {
                ok: false,
                detail: thrown("check", &e),
            },
        }
    }
}

impl Action for Js<'_> {
    fn supports(&self, kind: &str) -> bool {
        self.kind_supported(kind)
    }
    fn run(&mut self, c: &Call<'_>) -> Outcome {
        let detail = self
            .0
            .run(c.kind, &json(c.params), &env_json(c.env), c.cwd)
            .unwrap_or_else(|e| thrown("run", &e));
        Outcome {
            ok: detail.is_empty(),
            detail,
        }
    }
}

impl InstructionSource for Js<'_> {
    fn read(&self, file: &str) -> Result<Option<String>, String> {
        self.0.read(file).map_err(|e| thrown("read", &e))
    }
}

impl Matcher for Js<'_> {
    fn is_match(&self, pattern: &str, value: &str) -> Result<bool, String> {
        self.0
            .is_match(pattern, value)
            .map_err(|e| thrown("isMatch", &e))
    }
}

impl Clock for Js<'_> {
    fn now_ms(&self) -> u64 {
        self.0.now().unwrap_or(0.0) as u64
    }
}

impl Ids for Js<'_> {
    fn random(&mut self) -> u64 {
        (u64::from(self.random_u32()) << 32) | u64::from(self.random_u32())
    }
}

/// An smllm engine with an in-memory store.
#[wasm_bindgen]
pub struct Engine {
    engine: smllm_core::Engine,
    store: MemoryStore,
    host: JsHost,
}

fn err(e: impl core::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

#[wasm_bindgen]
impl Engine {
    /// Load compiled machines (`smllm compile` output).
    #[wasm_bindgen(constructor)]
    pub fn new(compiled: &str, host: JsHost) -> Result<Engine, JsError> {
        let config: Config = serde_json::from_str(compiled).map_err(err)?;
        Ok(Engine {
            engine: smllm_core::Engine::new(config),
            store: MemoryStore::default(),
            host,
        })
    }

    fn with<R>(&mut self, f: impl FnOnce(&smllm_core::Engine, &mut Host<'_>) -> R) -> R {
        let (mut g, mut a, mut i) = (Js(&self.host), Js(&self.host), Js(&self.host));
        let s = Js(&self.host);
        let mut host = Host {
            store: &mut self.store,
            guards: &mut g,
            actions: &mut a,
            source: &s,
            matcher: &s,
            clock: &s,
            ids: &mut i,
        };
        f(&self.engine, &mut host)
    }

    /// Guard and action kinds this host cannot run, as a JSON array.
    pub fn unsupported(&mut self) -> String {
        let js = Js(&self.host);
        json(&self.engine.unsupported(&js, &js))
    }

    /// Bind a session; returns the reply JSON (its `session` is the key).
    pub fn bind(
        &mut self,
        harness: &str,
        host_session: Option<String>,
        cwd: &str,
    ) -> Result<String, JsError> {
        let b = Bind {
            harness,
            host_session: host_session.as_deref(),
            cwd,
            configs: &[],
        };
        self.with(|e, h| e.bind(h, &b))
            .map(|r| json(&r))
            .map_err(err)
    }

    /// Read-only view.
    pub fn view(&mut self, key: &str) -> Result<String, JsError> {
        self.with(|e, h| e.view(h, key))
            .map(|r| json(&r))
            .map_err(err)
    }

    /// Where the session is, as the `smllm statusline --json` object (STL-8).
    // @zen-impl: STL-8_AC-1
    pub fn status(&mut self, key: &str) -> Result<String, JsError> {
        self.with(|e, h| e.status(h, key))
            .map(|s| json(&s))
            .map_err(err)
    }

    /// The events list.
    pub fn events(&mut self, key: &str) -> Result<String, JsError> {
        self.with(|e, h| e.menu(h, key))
            .map(|r| json(&r))
            .map_err(err)
    }

    /// Fire an event; `params` is a JSON object of strings.
    pub fn fire(
        &mut self,
        key: Option<String>,
        event: &str,
        params: &str,
    ) -> Result<String, JsError> {
        let map: serde_json::Map<String, serde_json::Value> = if params.is_empty() {
            Default::default()
        } else {
            serde_json::from_str(params).map_err(err)?
        };
        let mut ps = Vec::new();
        for (k, v) in map {
            match v {
                serde_json::Value::String(s) => ps.push((k, s)),
                _ => return Err(JsError::new(&format!("param {k} must be a string"))),
            }
        }
        let b = Bind {
            harness: "wasm",
            ..Bind::default()
        };
        self.with(|e, h| e.fire(h, key.as_deref(), event, &ps, &b))
            .map(|r| json(&r))
            .map_err(err)
    }

    /// The stop decision as JSON (TURN-4..6): `{"decision": "allow"}` = may
    /// stop; `{"decision": "block", "text"}` = hold the agent with the events
    /// list; `{"decision": "runaway", "text"}` = let it stop, showing the
    /// list to the user (the harness was already continuing because of a
    /// block, and no event was fired since).
    pub fn stop(&mut self, key: &str, stop_hook_active: bool) -> Result<String, JsError> {
        let decision = self
            .with(|e, h| e.stop(h, key, stop_hook_active))
            .map_err(err)?;
        let (name, text) = match decision {
            Stop::Allow => ("allow", None),
            Stop::Block(t) => ("block", Some(t)),
            Stop::Runaway(t) => ("runaway", Some(t)),
        };
        let mut out = serde_json::Map::new();
        out.insert("decision".into(), name.into());
        if let Some(t) = text {
            out.insert("text".into(), t.into());
        }
        Ok(json(&out))
    }

    /// A user prompt arrived.
    #[wasm_bindgen(js_name = promptSubmitted)]
    pub fn prompt_submitted(&mut self, key: &str) -> Result<(), JsError> {
        self.with(|e, h| e.prompt_submitted(h, key)).map_err(err)
    }

    /// Sessions, instances and history as JSON.
    #[wasm_bindgen(js_name = exportState)]
    pub fn export_state(&self) -> String {
        json(&self.store)
    }

    /// Replace the store from [`Engine::export_state`] JSON.
    #[wasm_bindgen(js_name = importState)]
    pub fn import_state(&mut self, state: &str) -> Result<(), JsError> {
        self.store = serde_json::from_str(state).map_err(err)?;
        Ok(())
    }
}
