//! An instance: one piece of work moving through a state machine (INST).
// @zen-component: INST-Records

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::prelude::*;
use crate::utils::SmallMap;

/// Machine id + instance id: how sessions point at instances.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct InstanceKey {
    /// State machine id.
    pub machine: String,
    /// Generated instance id.
    pub id: String,
}

/// Instance status (INST-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "camelCase")
)]
pub enum Status {
    /// Held by a session.
    Active,
    /// Put aside by `unmatched`; `resume` returns.
    Suspended,
    /// Put aside by `park`.
    Parked,
    /// Reached a final state; kept (INST-9).
    Completed,
}

impl Status {
    /// Lower-case name, as in text and JSON.
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Active => "active",
            Status::Suspended => "suspended",
            Status::Parked => "parked",
            Status::Completed => "completed",
        }
    }
}

/// An instance record.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct Instance {
    /// Generated id, permanent (INST-2).
    pub id: String,
    /// State machine id.
    pub machine: String,
    /// External ref, set once (INST-3).
    #[cfg_attr(feature = "serde", serde(default, rename = "ref"))]
    pub r#ref: Option<String>,
    /// Current (or saved) state.
    pub state: String,
    /// Status.
    pub status: Status,
    /// Holding session key.
    #[cfg_attr(feature = "serde", serde(default))]
    pub holder: Option<String>,
    /// Bumped on every write (INST-8).
    pub version: u64,
    /// Entries per state (ENG-3).
    #[cfg_attr(feature = "serde", serde(default))]
    pub visits: SmallMap<u32>,
    /// The state `unmatched` left for the fallback state (IDLE-2).
    #[cfg_attr(feature = "serde", serde(default))]
    pub interrupted: Option<String>,
    /// Created, unix ms.
    pub created: u64,
    /// Last change, unix ms.
    pub updated: u64,
}

impl Instance {
    /// The key sessions use to point here.
    pub fn key(&self) -> InstanceKey {
        InstanceKey {
            machine: self.machine.clone(),
            id: self.id.clone(),
        }
    }

    /// What the agent sees as the id param: the ref once set, else the id (INST-4).
    pub fn label(&self) -> &str {
        self.r#ref.as_deref().unwrap_or(&self.id)
    }

    /// Entries of `state` so far.
    pub fn visits(&self, state: &str) -> u32 {
        self.visits.get(state).copied().unwrap_or(0)
    }
}
