//! smllm-wasm: `smllm-core` for JavaScript hosts (browser or Node).
//!
//! The machines come in as `smllm compile` JSON. The JS host supplies guards,
//! actions, prompt files, pattern matching, time and randomness through one
//! object; sessions and instances live in an in-memory store the host can
//! export and import as JSON. Every call returns the core `Reply` as JSON.
// @zen-component: HOST-Wasm

use smllm_core::host::{
    Action, Call, Clock, Guard, Host, Ids, InstructionSource, Matcher, MemoryStore, Outcome,
};
use smllm_core::model::Config;
use smllm_core::{Bind, Stop};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// The JS host object. Every method is optional to call but must exist.
    pub type JsHost;

    /// Guard of a host kind: `true` passes.
    #[wasm_bindgen(method)]
    fn check(this: &JsHost, kind: &str, params: &str, env: &str, cwd: &str) -> bool;
    /// Action of a host kind: `""` = success, else the failure detail.
    #[wasm_bindgen(method)]
    fn run(this: &JsHost, kind: &str, params: &str, env: &str, cwd: &str) -> String;
    /// Host kinds supported (`"command"` …).
    #[wasm_bindgen(method)]
    fn supports(this: &JsHost, kind: &str) -> bool;
    /// A prompt file's text, or `undefined` when absent.
    #[wasm_bindgen(method)]
    fn read(this: &JsHost, file: &str) -> Option<String>;
    /// ECMA-262 `new RegExp(pattern).test(value)`.
    #[wasm_bindgen(method, js_name = isMatch)]
    fn is_match(this: &JsHost, pattern: &str, value: &str) -> bool;
    /// `Date.now()`.
    #[wasm_bindgen(method)]
    fn now(this: &JsHost) -> f64;
    /// A random 32-bit unsigned integer.
    #[wasm_bindgen(method)]
    fn random(this: &JsHost) -> u32;
}

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

impl Guard for Js<'_> {
    fn supports(&self, kind: &str) -> bool {
        self.0.supports(kind)
    }
    fn check(&mut self, c: &Call<'_>) -> Outcome {
        let ok = self
            .0
            .check(c.kind, &json(c.params), &env_json(c.env), c.cwd);
        Outcome {
            ok,
            detail: String::new(),
        }
    }
}

impl Action for Js<'_> {
    fn supports(&self, kind: &str) -> bool {
        self.0.supports(kind)
    }
    fn run(&mut self, c: &Call<'_>) -> Outcome {
        let detail = self.0.run(c.kind, &json(c.params), &env_json(c.env), c.cwd);
        Outcome {
            ok: detail.is_empty(),
            detail,
        }
    }
}

impl InstructionSource for Js<'_> {
    fn read(&self, file: &str) -> Result<Option<String>, String> {
        Ok(self.0.read(file))
    }
}

impl Matcher for Js<'_> {
    fn is_match(&self, pattern: &str, value: &str) -> Result<bool, String> {
        Ok(self.0.is_match(pattern, value))
    }
}

impl Clock for Js<'_> {
    fn now_ms(&self) -> u64 {
        self.0.now() as u64
    }
}

impl Ids for Js<'_> {
    fn random(&mut self) -> u64 {
        (u64::from(self.0.random()) << 32) | u64::from(self.0.random())
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

    /// The stop decision: `null` = may stop, else the events list text.
    pub fn stop(&mut self, key: &str, stop_hook_active: bool) -> Result<Option<String>, JsError> {
        match self
            .with(|e, h| e.stop(h, key, stop_hook_active))
            .map_err(err)?
        {
            Stop::Allow => Ok(None),
            Stop::Block(t) | Stop::Runaway(t) => Ok(Some(t)),
        }
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
