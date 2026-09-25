//! An in-memory [`Store`], for tests and hosts that persist a snapshot
//! (`smllm-wasm`).
// @zen-component: ENG-Host

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::host::{HostError, Store};
use crate::prelude::*;
use crate::record::{HistoryEntry, Instance, Session};
use crate::utils::SmallMap;

/// Everything in memory, keyed by strings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct MemoryStore {
    /// Sessions by key.
    pub sessions: SmallMap<Session>,
    /// `harness/host session id` → key.
    pub bindings: SmallMap<String>,
    /// `machine/id` → instance.
    pub instances: SmallMap<Instance>,
    /// `machine/id` → history.
    pub history: SmallMap<Vec<HistoryEntry>>,
}

fn join(a: &str, b: &str) -> String {
    format!("{a}/{b}")
}

impl Store for MemoryStore {
    fn session(&mut self, key: &str) -> Result<Option<Session>, HostError> {
        Ok(self.sessions.get(key).cloned())
    }

    fn put_session(&mut self, session: &Session) -> Result<(), HostError> {
        self.sessions.insert(session.key.clone(), session.clone());
        Ok(())
    }

    fn binding(&mut self, harness: &str, host_session: &str) -> Result<Option<String>, HostError> {
        Ok(self.bindings.get(&join(harness, host_session)).cloned())
    }

    fn put_binding(
        &mut self,
        harness: &str,
        host_session: &str,
        key: &str,
    ) -> Result<(), HostError> {
        self.bindings
            .insert(join(harness, host_session), key.to_string());
        Ok(())
    }

    fn instance(&mut self, machine: &str, id: &str) -> Result<Option<Instance>, HostError> {
        Ok(self.instances.get(&join(machine, id)).cloned())
    }

    fn instances(&mut self, machine: &str) -> Result<Vec<Instance>, HostError> {
        Ok(self
            .instances
            .iter()
            .filter(|(_, i)| i.machine == machine)
            .map(|(_, i)| i.clone())
            .collect())
    }

    fn put_instance(&mut self, instance: &Instance) -> Result<(), HostError> {
        let key = join(&instance.machine, &instance.id);
        let stored = self.instances.get(&key).map_or(0, |i| i.version);
        if instance.version != stored + 1 {
            return Err(HostError::Conflict);
        }
        self.instances.insert(key, instance.clone());
        Ok(())
    }

    fn append_history(
        &mut self,
        machine: &str,
        id: &str,
        entry: &HistoryEntry,
    ) -> Result<(), HostError> {
        let key = join(machine, id);
        match self.history.get_mut(&key) {
            Some(list) => list.push(entry.clone()),
            None => {
                self.history.insert(key, vec![entry.clone()]);
            }
        }
        Ok(())
    }
}
