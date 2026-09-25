//! What a host supplies: the core has no IO, time or randomness (NFR-1).
// @zen-component: ENG-Host

use crate::model::Value;
use crate::prelude::*;
use crate::record::{HistoryEntry, Instance, Session};
use crate::utils::SmallMap;

/// A store failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    /// Another writer changed the instance first (INST-8).
    Conflict,
    /// Anything else, as a message.
    Other(String),
}

impl core::fmt::Display for HostError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            HostError::Conflict => f.write_str("the instance was changed by another session"),
            HostError::Other(m) => f.write_str(m),
        }
    }
}

/// Sessions, bindings, instances and history.
pub trait Store {
    /// The session with `key`.
    fn session(&mut self, key: &str) -> Result<Option<Session>, HostError>;
    /// Save a session.
    fn put_session(&mut self, session: &Session) -> Result<(), HostError>;
    /// The key bound to a harness session id.
    fn binding(&mut self, harness: &str, host_session: &str) -> Result<Option<String>, HostError>;
    /// Bind a harness session id to a key.
    fn put_binding(
        &mut self,
        harness: &str,
        host_session: &str,
        key: &str,
    ) -> Result<(), HostError>;
    /// The instance `id` of `machine`.
    fn instance(&mut self, machine: &str, id: &str) -> Result<Option<Instance>, HostError>;
    /// Every instance of `machine`.
    fn instances(&mut self, machine: &str) -> Result<Vec<Instance>, HostError>;
    /// Save an instance whose `version` was bumped by one from the stored copy
    /// (or is 1 for a new one); anything else is [`HostError::Conflict`].
    fn put_instance(&mut self, instance: &Instance) -> Result<(), HostError>;
    /// Append a history entry.
    fn append_history(
        &mut self,
        machine: &str,
        id: &str,
        entry: &HistoryEntry,
    ) -> Result<(), HostError>;
}

/// A guard or action invocation handed to the host.
pub struct Call<'a> {
    /// Machine id.
    pub machine: &'a str,
    /// `type`.
    pub kind: &'a str,
    /// `params`.
    pub params: &'a SmallMap<Value>,
    /// Environment (DEC-6, ACT-4).
    pub env: &'a [(String, String)],
    /// The session's working dir (DEC-7).
    pub cwd: &'a str,
}

/// How a host guard or action went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Guard: true. Action: succeeded.
    pub ok: bool,
    /// Exit status, reason and output tail, for the trace (DEC-8).
    pub detail: String,
}

/// Host guard kinds (`command`).
pub trait Guard {
    /// Whether this host evaluates `kind` (NFR-9).
    fn supports(&self, kind: &str) -> bool;
    /// Evaluate a guard.
    fn check(&mut self, call: &Call<'_>) -> Outcome;
}

/// Host action kinds (`command`).
pub trait Action {
    /// Whether this host runs `kind` (ACT-6).
    fn supports(&self, kind: &str) -> bool;
    /// Run an action.
    fn run(&mut self, call: &Call<'_>) -> Outcome;
}

/// Reads prompt files (ENG-4).
pub trait InstructionSource {
    /// The text of `file`; `Ok(None)` when it does not exist.
    fn read(&self, file: &str) -> Result<Option<String>, String>;
}

/// Matches param `pattern`s (JSON Schema / ECMA-262 regex).
pub trait Matcher {
    /// Whether `value` matches `pattern`.
    fn is_match(&self, pattern: &str, value: &str) -> Result<bool, String>;
}

/// Wall-clock time.
pub trait Clock {
    /// Unix time, ms.
    fn now_ms(&self) -> u64;
}

/// Randomness for keys and ids.
pub trait Ids {
    /// 64 random bits.
    fn random(&mut self) -> u64;
}

/// Everything a host supplies, borrowed for one engine call.
pub struct Host<'a> {
    /// Storage.
    pub store: &'a mut dyn Store,
    /// Host guards.
    pub guards: &'a mut dyn Guard,
    /// Host actions.
    pub actions: &'a mut dyn Action,
    /// Prompt files.
    pub source: &'a dyn InstructionSource,
    /// Patterns.
    pub matcher: &'a dyn Matcher,
    /// Time.
    pub clock: &'a dyn Clock,
    /// Randomness.
    pub ids: &'a mut dyn Ids,
}
