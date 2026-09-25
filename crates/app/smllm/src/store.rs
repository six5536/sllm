//! The file store: sessions and bindings under the user's state dir,
//! instances and history in `state/` beside their config; atomic writes, a
//! file lock and instance versions (STO, INST-8).
// @zen-component: STO-FileStore

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use smllm_core::host::{HostError, Store};
use smllm_core::record::{HistoryEntry, Instance, Session};

/// Sessions under `user`, instances under each machine's state dir.
pub struct FsStore {
    user: PathBuf,
    machines: HashMap<String, PathBuf>,
}

fn other(e: impl std::fmt::Display) -> HostError {
    HostError::Other(e.to_string())
}

/// Write via a temp file + rename, so readers never see half a file.
// @zen-impl: STO-3_AC-1
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?;
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.tmp{}", std::process::id()));
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, HostError> {
    match fs::read_to_string(path) {
        Ok(t) => serde_json::from_str(&t)
            .map(Some)
            .map_err(|e| other(format!("{}: {e}", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(other(format!("{}: {e}", path.display()))),
    }
}

/// A file name from arbitrary text (harness session ids).
fn safe(name: &str) -> String {
    // Injective: `_` and other bytes are %-escaped, so `a/b` and `a_b` differ.
    let mut out = String::with_capacity(name.len());
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl FsStore {
    /// A store with sessions under `user`; `machines` maps machine id → state dir.
    pub fn new(user: PathBuf, machines: HashMap<String, PathBuf>) -> Self {
        Self { user, machines }
    }

    fn session_path(&self, key: &str) -> PathBuf {
        self.user
            .join("sessions")
            .join(format!("{}.json", safe(key)))
    }

    fn machine_dir(&self, machine: &str) -> Result<PathBuf, HostError> {
        self.machines
            .get(machine)
            .map(|d| d.join(safe(machine)))
            .ok_or_else(|| other(format!("state machine {machine} is not configured")))
    }

    /// Every session, newest first.
    pub fn sessions(&self) -> Result<Vec<Session>, HostError> {
        let dir = self.user.join("sessions");
        let mut out: Vec<Session> = Vec::new();
        let Ok(rd) = fs::read_dir(&dir) else {
            return Ok(out);
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "json")
                && let Some(s) = read_json(&p)?
            {
                out.push(s);
            }
        }
        out.sort_by(|a, b| b.last_active.cmp(&a.last_active).then(a.key.cmp(&b.key)));
        Ok(out)
    }

    /// The history of an instance.
    pub fn history(&self, machine: &str, id: &str) -> Result<Vec<HistoryEntry>, HostError> {
        let p = self
            .machine_dir(machine)?
            .join(format!("{}.history.jsonl", safe(id)));
        let text = match fs::read_to_string(&p) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(other(e)),
        };
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).map_err(other))
            .collect()
    }

    /// Keep instance state out of git (O6: private in v1).
    fn ignore_state(&self, machine_dir: &Path) {
        if let Some(state) = machine_dir.parent() {
            let gi = state.join(".gitignore");
            if !gi.exists() {
                let _ = fs::create_dir_all(state);
                let _ = fs::write(gi, "# smllm instance state: private to this checkout.\n*\n");
            }
        }
    }
}

impl Store for FsStore {
    fn session(&mut self, key: &str) -> Result<Option<Session>, HostError> {
        read_json(&self.session_path(key))
    }

    fn put_session(&mut self, session: &Session) -> Result<(), HostError> {
        let text = serde_json::to_string_pretty(session).map_err(other)?;
        write_atomic(&self.session_path(&session.key), &text).map_err(other)
    }

    fn binding(&mut self, harness: &str, host_session: &str) -> Result<Option<String>, HostError> {
        let p = self
            .user
            .join("bindings")
            .join(safe(harness))
            .join(safe(host_session));
        match fs::read_to_string(p) {
            Ok(k) => Ok(Some(k.trim().to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(other(e)),
        }
    }

    fn put_binding(
        &mut self,
        harness: &str,
        host_session: &str,
        key: &str,
    ) -> Result<(), HostError> {
        let p = self
            .user
            .join("bindings")
            .join(safe(harness))
            .join(safe(host_session));
        write_atomic(&p, key).map_err(other)
    }

    fn instance(&mut self, machine: &str, id: &str) -> Result<Option<Instance>, HostError> {
        let Ok(dir) = self.machine_dir(machine) else {
            return Ok(None);
        };
        read_json(&dir.join(format!("{}.json", safe(id))))
    }

    fn instances(&mut self, machine: &str) -> Result<Vec<Instance>, HostError> {
        let Ok(dir) = self.machine_dir(machine) else {
            return Ok(Vec::new());
        };
        let mut out: Vec<Instance> = Vec::new();
        let Ok(rd) = fs::read_dir(&dir) else {
            return Ok(out);
        };
        for e in rd.flatten() {
            let p = e.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if name.ends_with(".json")
                && !name.ends_with(".history.jsonl")
                && let Some(i) = read_json(&p)?
            {
                out.push(i);
            }
        }
        out.sort_by(|a: &Instance, b| a.id.cmp(&b.id));
        Ok(out)
    }

    // @zen-impl: INST-8_AC-2
    fn put_instance(&mut self, instance: &Instance) -> Result<(), HostError> {
        let dir = self.machine_dir(&instance.machine)?;
        fs::create_dir_all(&dir).map_err(other)?;
        self.ignore_state(&dir);
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join(".lock"))
            .map_err(other)?;
        lock.lock().map_err(other)?;
        let path = dir.join(format!("{}.json", safe(&instance.id)));
        let stored: Option<Instance> = read_json(&path)?;
        let stored = stored.map_or(0, |i| i.version);
        let result = if instance.version != stored + 1 {
            Err(HostError::Conflict)
        } else {
            let text = serde_json::to_string_pretty(instance).map_err(other)?;
            write_atomic(&path, &text).map_err(other)
        };
        let _ = lock.unlock();
        result
    }

    fn append_history(
        &mut self,
        machine: &str,
        id: &str,
        entry: &HistoryEntry,
    ) -> Result<(), HostError> {
        let dir = self.machine_dir(machine)?;
        fs::create_dir_all(&dir).map_err(other)?;
        let mut line = serde_json::to_string(entry).map_err(other)?;
        line.push('\n');
        let mut f: File = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(format!("{}.history.jsonl", safe(id))))
            .map_err(other)?;
        f.write_all(line.as_bytes()).map_err(other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smllm_core::SmallMap;
    use smllm_core::record::Status;

    fn inst(v: u64) -> Instance {
        Instance {
            id: "i-1".into(),
            machine: "dev".into(),
            r#ref: None,
            state: "A".into(),
            status: Status::Active,
            holder: None,
            version: v,
            visits: SmallMap::new(),
            interrupted: None,
            created: 0,
            updated: 0,
        }
    }

    // @zen-test: INST-8_AC-2
    // @zen-test: STO-3_AC-1
    #[test]
    fn versions_guard_instance_writes_and_history_appends() {
        let d = std::env::temp_dir().join(format!("smllm-store-{}", std::process::id()));
        let mut s = FsStore::new(
            d.join("user"),
            [("dev".to_string(), d.join("proj/state"))].into(),
        );
        assert_eq!(s.put_instance(&inst(2)), Err(HostError::Conflict));
        s.put_instance(&inst(1)).unwrap();
        assert_eq!(s.put_instance(&inst(1)), Err(HostError::Conflict));
        s.put_instance(&inst(2)).unwrap();
        assert_eq!(s.instance("dev", "i-1").unwrap().unwrap().version, 2);
        assert_eq!(s.instances("dev").unwrap().len(), 1);
        assert!(d.join("proj/state/.gitignore").is_file());
        s.append_history(
            "dev",
            "i-1",
            &HistoryEntry {
                event: "a".into(),
                ..Default::default()
            },
        )
        .unwrap();
        s.append_history(
            "dev",
            "i-1",
            &HistoryEntry {
                event: "b".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(s.history("dev", "i-1").unwrap().len(), 2);
        assert!(s.instance("nope", "x").unwrap().is_none());
        assert!(s.instances("nope").unwrap().is_empty());
        s.put_binding("claude", "a/b", "sm-1").unwrap();
        assert_eq!(s.binding("claude", "a/b").unwrap().as_deref(), Some("sm-1"));
        assert!(s.binding("claude", "zz").unwrap().is_none());
        assert!(
            s.binding("claude", "a_b").unwrap().is_none(),
            "a/b and a_b are distinct"
        );
        assert_eq!(safe("sm-1"), "sm-1");
        let sess = Session {
            key: "sm-1".into(),
            ..Default::default()
        };
        s.put_session(&sess).unwrap();
        assert_eq!(s.session("sm-1").unwrap(), Some(sess));
        assert_eq!(s.sessions().unwrap().len(), 1);
        fs::remove_dir_all(d).ok();
    }
}
