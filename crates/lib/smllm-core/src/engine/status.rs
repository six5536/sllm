//! Where a session is, as data, for status lines (STL-5, STL-8).
// @zen-component: STL-Status

#[cfg(feature = "serde")]
use serde::Serialize;

use crate::engine::{Engine, Error};
use crate::host::Host;
use crate::model::Machine;
use crate::prelude::*;
use crate::record::{Instance, Status};

/// A session's status: the `smllm statusline --json` object (STL-5).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize), serde(rename_all = "camelCase"))]
pub struct SessionStatus {
    /// The session key.
    pub session: String,
    /// True when the session holds no instance.
    pub idle: bool,
    /// The held instance's state machine.
    pub machine: Option<String>,
    /// Its current state.
    pub state: Option<String>,
    /// Entries of that state so far.
    pub visit: Option<u32>,
    /// The agent fired `yield` since the last user prompt.
    pub yielded: bool,
    /// The held instance.
    pub instance: Option<InstanceStatus>,
    /// The instance put aside by `unmatched`.
    pub suspended: Option<InstanceStatus>,
    /// Parked instances of the configured state machines.
    pub parked: u32,
}

/// An instance, as a status shows it (STL-5_AC-2).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize), serde(rename_all = "camelCase"))]
pub struct InstanceStatus {
    /// State machine id.
    pub machine: String,
    /// What the machine calls an instance (`issue`).
    pub kind: String,
    /// Generated id.
    pub id: String,
    /// External ref, once set.
    #[cfg_attr(feature = "serde", serde(rename = "ref"))]
    pub r#ref: Option<String>,
    /// The ref once set, else the id.
    pub label: String,
    /// `active`, `suspended`, `parked` or `completed`.
    pub status: String,
}

impl InstanceStatus {
    fn new(machine: &Machine, inst: &Instance) -> Self {
        Self {
            machine: machine.id.clone(),
            kind: machine.instance.kind.clone(),
            id: inst.id.clone(),
            r#ref: inst.r#ref.clone(),
            label: inst.label().to_string(),
            status: inst.status.as_str().to_string(),
        }
    }
}

impl Engine {
    /// Where session `key` is; reads only, never binds, writes or runs a
    /// command (STL-3). A held instance that moved or whose machine is not
    /// configured reads as idle, as the view shows it.
    // @zen-impl: STL-3_AC-1
    // @zen-impl: STL-5_AC-1
    // @zen-impl: STL-5_AC-2
    // @zen-impl: STL-8_AC-1
    pub fn status(&self, host: &mut Host<'_>, key: &str) -> Result<SessionStatus, Error> {
        let session = host
            .store
            .session(key)?
            .ok_or_else(|| Error::UnknownSession(key.to_string()))?;
        let config = self.config();
        let mut read = |k: &crate::record::InstanceKey, status: Status| -> Result<_, Error> {
            let Some(machine) = config.machine(&k.machine) else {
                return Ok(None);
            };
            Ok(host
                .store
                .instance(&k.machine, &k.id)?
                .filter(|i| i.status == status && i.holder.as_deref() == Some(key))
                .map(|i| (machine, i)))
        };
        let held = match &session.holding {
            Some(k) => read(k, Status::Active)?,
            None => None,
        };
        let suspended = match &session.suspended {
            Some(k) => read(k, Status::Suspended)?.map(|(m, i)| InstanceStatus::new(m, &i)),
            None => None,
        };
        let mut parked = 0;
        for m in &config.machines {
            parked += host
                .store
                .instances(&m.id)?
                .iter()
                .filter(|i| i.status == Status::Parked)
                .count() as u32;
        }
        Ok(SessionStatus {
            session: session.key,
            idle: held.is_none(),
            machine: held.as_ref().map(|(m, _)| m.id.clone()),
            state: held.as_ref().map(|(_, i)| i.state.clone()),
            visit: held.as_ref().map(|(_, i)| i.visits(&i.state)),
            yielded: session.yielded,
            instance: held.as_ref().map(|(m, i)| InstanceStatus::new(m, i)),
            suspended,
            parked,
        })
    }
}
