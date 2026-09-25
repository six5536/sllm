//! Guards and actions, XState-style `{type, params}` (CFG-4, CFG-5).
// @zen-component: ENG-Model

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::prelude::*;
use crate::utils::SmallMap;

/// A guard/action param value, as written in the machine file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(untagged))]
pub enum Value {
    /// A string (`run: "cargo test"`, `cwd: ...`).
    Str(String),
    /// An integer (`timeoutSecs: 300`).
    Int(i64),
    /// A boolean.
    Bool(bool),
    /// A list of strings (`run: [cargo, test]`: exec, no shell).
    List(Vec<String>),
}

impl Value {
    /// The string, if this is one.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    /// The integer, if this is one.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }
}

/// A guard on a transition.
// @zen-impl: DEC-3_AC-1
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "camelCase")
)]
pub enum GuardDef {
    /// `visits`: true when `state` has been entered at least `at_least` times
    /// (counting the current entry). Evaluated by the core.
    Visits {
        /// The state whose entries are counted.
        state: String,
        /// The threshold.
        at_least: u32,
    },
    /// A kind the host evaluates (`command`).
    Host {
        /// The guard `type`.
        kind: String,
        /// Its `params`.
        params: SmallMap<Value>,
    },
}

/// Prompt text, inline or from a file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "camelCase")
)]
pub enum Prompt {
    /// Inline text.
    Text(String),
    /// A file, read through the host's `InstructionSource` at request time.
    File(String),
    /// A file read only if it exists: the implied `enter-<STATE>.md`.
    DefaultFile(String),
}

/// An action in `entry`, `exit` or a transition's `actions`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "camelCase")
)]
pub enum ActionDef {
    /// Text for the agent, gathered into `<instructions>` (ACT-2).
    Prompt(Prompt),
    /// Set the instance's ref from the event's ref param (INST-3).
    SetRef,
    /// A kind the host runs (`command`).
    Host {
        /// The action `type`.
        kind: String,
        /// Its `params`.
        params: SmallMap<Value>,
    },
}
