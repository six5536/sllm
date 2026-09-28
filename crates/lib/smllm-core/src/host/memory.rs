//! An in-memory [`Store`], for tests and `smllm-wasm` (which hands each saved
//! record to its JS host). Two indexes, derived from the instances and never
//! saved, answer lookups without scanning or cloning every instance
//! (PLAN-008 D8-6).
// @zen-component: ENG-Host

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::host::{HostError, Store};
use crate::prelude::*;
use crate::record::{HistoryEntry, Instance, Session, Status};
use crate::utils::SmallMap;

/// Everything in memory, keyed by strings. Build one from saved state with
/// [`MemoryStore::from_parts`], which builds the indexes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "camelCase", from = "Saved")
)]
pub struct MemoryStore {
    /// Sessions by key.
    sessions: SmallMap<Session>,
    /// `harness/host session id` → key.
    bindings: SmallMap<String>,
    /// `machine/id` → instance.
    instances: SmallMap<Instance>,
    /// `machine/id` → history. Left out of the JSON while empty, as it is
    /// for hosts that keep the log themselves (`smllm-wasm`).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "SmallMap::is_empty"))]
    history: SmallMap<Vec<HistoryEntry>>,
    /// `machine\0ref` → id.
    #[cfg_attr(feature = "serde", serde(skip))]
    by_ref: SmallMap<String>,
    /// `machine\0status\0updated\0label\0id` → nothing, `updated` as 16
    /// hex digits: a machine's instances of one status are a contiguous run,
    /// by `updated`, then label, so a count is its length and the newest are
    /// its last groups (IDLE-7's order without a sort, however many tie).
    #[cfg_attr(feature = "serde", serde(skip))]
    by_status: SmallMap<()>,
}

/// The saved form: everything but the indexes.
#[cfg(feature = "serde")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    sessions: SmallMap<Session>,
    bindings: SmallMap<String>,
    instances: SmallMap<Instance>,
    #[serde(default)]
    history: SmallMap<Vec<HistoryEntry>>,
}

#[cfg(feature = "serde")]
impl From<Saved> for MemoryStore {
    fn from(s: Saved) -> Self {
        Self::from_parts(s.sessions, s.bindings, s.instances, s.history)
    }
}

fn join(a: &str, b: &str) -> String {
    format!("{a}/{b}")
}

fn ref_key(machine: &str, r#ref: &str) -> String {
    format!("{machine}\0{ref}")
}

fn status_prefix(machine: &str, status: Status) -> String {
    format!("{machine}\0{}\0", status.as_str())
}

fn status_key(i: &Instance) -> String {
    format!(
        "{}{:016x}\0{}\0{}",
        status_prefix(&i.machine, i.status),
        i.updated,
        i.label(),
        i.id
    )
}

/// A status key's `updated` group, after its prefix: 16 digits and `\0`.
const TIME_LEN: usize = 17;

/// The id at the end of a status key.
fn id_of(key: &str) -> &str {
    key.rsplit('\0').next().unwrap_or_default()
}

impl MemoryStore {
    /// A store holding saved state, with its indexes built.
    pub fn from_parts(
        sessions: SmallMap<Session>,
        bindings: SmallMap<String>,
        instances: SmallMap<Instance>,
        history: SmallMap<Vec<HistoryEntry>>,
    ) -> Self {
        let mut store = Self {
            sessions,
            bindings,
            instances,
            history,
            ..Self::default()
        };
        let all: Vec<Instance> = store.instances.iter().map(|(_, i)| i.clone()).collect();
        for i in &all {
            store.index(i);
        }
        store
    }

    /// Sessions by key.
    pub fn sessions(&self) -> &SmallMap<Session> {
        &self.sessions
    }

    /// `harness/host session id` → key.
    pub fn bindings(&self) -> &SmallMap<String> {
        &self.bindings
    }

    /// `machine/id` → instance (named apart from [`Store::instances`]).
    pub fn all_instances(&self) -> &SmallMap<Instance> {
        &self.instances
    }

    /// `machine/id` → history.
    pub fn history(&self) -> &SmallMap<Vec<HistoryEntry>> {
        &self.history
    }

    fn index(&mut self, i: &Instance) {
        if let Some(r) = &i.r#ref {
            self.by_ref.insert(ref_key(&i.machine, r), i.id.clone());
        }
        self.by_status.insert(status_key(i), ());
    }

    fn unindex(&mut self, i: &Instance) {
        if let Some(r) = &i.r#ref {
            self.by_ref.remove(&ref_key(&i.machine, r));
        }
        self.by_status.remove(&status_key(i));
    }

    /// The instances of `machine` with `status`, borrowed, oldest first.
    fn with_status(&self, machine: &str, status: Status) -> Vec<&Instance> {
        self.by_status
            .prefixed(&status_prefix(machine, status))
            .iter()
            .filter_map(|(k, _)| self.instances.get(&join(machine, id_of(k))))
            .collect()
    }
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
        let prefix = join(machine, "");
        Ok(self
            .instances
            .prefixed(&prefix)
            .iter()
            .map(|(_, i)| i.clone())
            .collect())
    }

    fn instances_with(
        &mut self,
        machine: &str,
        status: Status,
    ) -> Result<Vec<Instance>, HostError> {
        Ok(self
            .with_status(machine, status)
            .into_iter()
            .cloned()
            .collect())
    }

    fn instance_by_ref(
        &mut self,
        machine: &str,
        r#ref: &str,
    ) -> Result<Option<Instance>, HostError> {
        Ok(self
            .by_ref
            .get(&ref_key(machine, r#ref))
            .and_then(|id| self.instances.get(&join(machine, id)))
            .cloned())
    }

    fn count(&mut self, machine: &str, status: Status) -> Result<usize, HostError> {
        Ok(self
            .by_status
            .prefixed(&status_prefix(machine, status))
            .len())
    }

    fn recent(
        &mut self,
        machine: &str,
        status: Status,
        limit: usize,
    ) -> Result<Vec<Instance>, HostError> {
        // Newest group (same `updated`) first; within one, label order is
        // key order, so each group is read forward from its start.
        let prefix = status_prefix(machine, status);
        let run = self.by_status.prefixed(&prefix);
        let mut out = Vec::new();
        let mut end = run.len();
        while end > 0 && out.len() < limit {
            let group = &run[end - 1].0[..prefix.len() + TIME_LEN];
            let start = run[..end].partition_point(|(k, _)| k.as_str() < group);
            for (k, _) in &run[start..end] {
                if out.len() == limit {
                    break;
                }
                out.extend(self.instances.get(&join(machine, id_of(k))).cloned());
            }
            end = start;
        }
        Ok(out)
    }

    /// The version rule (INST-8), and one ref per machine (INST-3): a ref new
    /// to the instance that another instance of the machine holds, or that is
    /// another instance's id, is a conflict, as in the file store.
    fn put_instance(&mut self, instance: &Instance) -> Result<(), HostError> {
        let key = join(&instance.machine, &instance.id);
        let stored = self.instances.get(&key).cloned();
        if instance.version != stored.as_ref().map_or(0, |i| i.version) + 1 {
            return Err(HostError::Conflict);
        }
        let stored_ref = stored.as_ref().and_then(|i| i.r#ref.as_deref());
        if let Some(r) = instance.r#ref.as_deref()
            && stored_ref != Some(r)
        {
            let holder = self.by_ref.get(&ref_key(&instance.machine, r));
            let taken = holder.is_some_and(|id| *id != instance.id)
                || (r != instance.id && self.instances.contains_key(&join(&instance.machine, r)));
            if taken {
                return Err(HostError::Conflict);
            }
        }
        if let Some(old) = &stored {
            self.unindex(old);
        }
        self.index(instance);
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
