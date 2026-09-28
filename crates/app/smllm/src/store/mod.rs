//! The file store: sessions and bindings under the user's state dir,
//! instances and history in `state/` beside their config ([`instances`]);
//! atomic writes, a file lock and instance versions (STO, INST-8).
// @zen-component: STO-FileStore

mod instances;

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use agent_harness_kit::fs as fs_kit;
use smllm_core::host::{HostError, Store};
use smllm_core::record::{HistoryEntry, Instance, Session, Status};

/// Sessions under `user`, instances under each machine's state dir.
pub struct FsStore {
    user: PathBuf,
    machines: HashMap<String, PathBuf>,
}

fn other(e: impl std::fmt::Display) -> HostError {
    HostError::Other(e.to_string())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, HostError> {
    let Some(text) = fs_kit::read_text(path).map_err(other)? else {
        return Ok(None);
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| other(format!("{}: {e}", path.display())))
}

/// Write via a temp file + rename, so readers never see half a file.
// @zen-impl: STO-3_AC-1
fn write(path: &Path, text: &str) -> Result<(), HostError> {
    fs_kit::write_atomic(path, text).map_err(other)
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

    /// Sessions and bindings only: no machine is configured.
    pub fn user() -> crate::error::Result<Self> {
        Ok(Self::new(crate::paths::user_state_dir()?, HashMap::new()))
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
        let dir = self.machine_dir(machine)?;
        let Some(text) = fs_kit::read_text(&instances::history_path(&dir, id)).map_err(other)?
        else {
            return Ok(Vec::new());
        };
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).map_err(other))
            .collect()
    }

    /// Every instance file of `machine`, both shelves: the readable ones
    /// by id, and the unreadable ones with why (`validate` reports those,
    /// STO-1).
    pub fn scan(&self, machine: &str) -> (Vec<Instance>, Vec<(PathBuf, String)>) {
        match self.machine_dir(machine) {
            Ok(dir) => instances::scan(&dir, None),
            Err(_) => (Vec::new(), Vec::new()),
        }
    }

    /// Ref marker files of `machine` that cannot be read, with why
    /// (`validate` reports those; smllm reads refs without them).
    pub fn unreadable_markers(&self, machine: &str) -> Vec<(PathBuf, String)> {
        self.machine_dir(machine)
            .map(|dir| instances::unreadable_markers(&dir))
            .unwrap_or_default()
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
        write(&self.session_path(&session.key), &text)
    }

    fn binding(&mut self, harness: &str, host_session: &str) -> Result<Option<String>, HostError> {
        let p = self
            .user
            .join("bindings")
            .join(safe(harness))
            .join(safe(host_session));
        Ok(fs_kit::read_text(&p)
            .map_err(other)?
            .map(|k| k.trim().to_string()))
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
        write(&p, key)
    }

    fn instance(&mut self, machine: &str, id: &str) -> Result<Option<Instance>, HostError> {
        let Ok(dir) = self.machine_dir(machine) else {
            return Ok(None);
        };
        Ok(instances::find(&dir, id)?.map(|(_, i)| i))
    }

    /// The machine's readable instances. An unreadable or corrupt file is
    /// skipped, so one bad file cannot break every idle list and status
    /// line; `validate` reports it ([`FsStore::scan`]).
    fn instances(&mut self, machine: &str) -> Result<Vec<Instance>, HostError> {
        Ok(self.scan(machine).0)
    }

    /// One shelf: a paused list reads no completed instance.
    fn instances_with(
        &mut self,
        machine: &str,
        status: Status,
    ) -> Result<Vec<Instance>, HostError> {
        Ok(match self.machine_dir(machine) {
            Ok(dir) => instances::scan(&dir, Some(status)).0,
            Err(_) => Vec::new(),
        })
    }

    /// One marker file.
    fn instance_by_ref(
        &mut self,
        machine: &str,
        r#ref: &str,
    ) -> Result<Option<Instance>, HostError> {
        let Ok(dir) = self.machine_dir(machine) else {
            return Ok(None);
        };
        instances::by_ref(&dir, r#ref)
    }

    fn put_instance(&mut self, instance: &Instance) -> Result<(), HostError> {
        let dir = self.machine_dir(&instance.machine)?;
        self.ignore_state(&dir);
        instances::put(&dir, instance)
    }

    fn append_history(
        &mut self,
        machine: &str,
        id: &str,
        entry: &HistoryEntry,
    ) -> Result<(), HostError> {
        let dir = self.machine_dir(machine)?;
        let path = instances::history_path(&dir, id);
        fs::create_dir_all(path.parent().unwrap_or(&dir)).map_err(other)?;
        let mut line = serde_json::to_string(entry).map_err(other)?;
        line.push('\n');
        let mut f: File = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
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
            resume_state: None,
            created: 0,
            updated: 0,
        }
    }

    // @zen-test: INST-8_AC-2
    // @zen-test: INST-3_AC-1
    // @zen-test: STO-3_AC-1
    #[test]
    fn versions_guard_instance_writes_and_history_appends() {
        let d = crate::test_support::temp_dir("store");
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
        // A second new instance with a ref already taken is a conflict.
        let mut a = inst(3);
        a.r#ref = Some("GH-1".into());
        s.put_instance(&a).unwrap();
        let mut b = inst(1);
        b.id = "i-2".into();
        b.r#ref = Some("GH-1".into());
        assert_eq!(s.put_instance(&b), Err(HostError::Conflict));
        b.r#ref = Some("i-1".into());
        assert_eq!(s.put_instance(&b), Err(HostError::Conflict));
        b.r#ref = Some("GH-2".into());
        s.put_instance(&b).unwrap();
        // Only a new ref is checked against the others (PLAN-004 P-8): a
        // clash made by hand does not stop the instance from saving.
        let mut clash = b.clone();
        clash.id = "i-3".into();
        fs::write(
            d.join("proj/state/dev/open/i-3.json"),
            serde_json::to_string(&clash).unwrap(),
        )
        .unwrap();
        b.version += 1;
        s.put_instance(&b).unwrap();
        fs::remove_file(d.join("proj/state/dev/open/i-3.json")).unwrap();
        // A corrupt instance file is skipped, and reported (PLAN-003 F5).
        fs::write(d.join("proj/state/dev/open/i-bad.json"), "{ nope").unwrap();
        assert_eq!(s.instances("dev").unwrap().len(), 2);
        let bad = s.scan("dev").1;
        assert_eq!(bad.len(), 1);
        assert!(bad[0].0.ends_with("i-bad.json"), "{bad:?}");
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

    // Live instances in `open/`, completed ones in `done/`, refs as marker
    // files, histories in `history/` (PLAN-005); reads write nothing
    // (PLAN-006 D6-1).
    // @zen-test: INST-3_AC-1
    // @zen-test: INST-4_AC-1
    #[test]
    fn shelves_and_markers() {
        let d = crate::test_support::temp_dir("shelves");
        let dev = d.join("state/dev");
        let mut s = FsStore::new(
            d.join("user"),
            [("dev".to_string(), d.join("state"))].into(),
        );
        // A machine with no state yet reads as empty, and writes nothing.
        assert!(s.instances_with("dev", Status::Paused).unwrap().is_empty());
        assert!(s.instance_by_ref("dev", "R1").unwrap().is_none());
        assert!(s.history("dev", "i-a").unwrap().is_empty());
        assert!(!d.join("state").exists());
        let mut a = inst(1);
        a.id = "i-a".into();
        a.r#ref = Some("R1".into());
        let mut b = inst(1);
        b.id = "i-b".into();
        b.r#ref = Some("R2".into());
        b.status = Status::Completed;
        s.put_instance(&a).unwrap();
        s.put_instance(&b).unwrap();
        s.append_history("dev", "i-a", &HistoryEntry::default())
            .unwrap();
        assert!(dev.join("open/i-a.json").is_file() && dev.join("done/i-b.json").is_file());
        assert!(dev.join("history/i-a.jsonl").is_file());
        assert_eq!(s.history("dev", "i-a").unwrap().len(), 1);
        assert_eq!(s.instance_by_ref("dev", "R2").unwrap().unwrap().id, "i-b");
        fs::write(dev.join("open/i-c.json"), "{ nope").unwrap();
        assert_eq!(s.instances("dev").unwrap().len(), 2);
        assert_eq!(s.scan("dev").1.len(), 1);
        fs::remove_file(dev.join("open/i-c.json")).unwrap();

        // Completing moves the file; a paused list never reads `done/`.
        a.version = 2;
        a.status = Status::Completed;
        s.put_instance(&a).unwrap();
        assert!(dev.join("done/i-a.json").is_file() && !dev.join("open/i-a.json").exists());
        let mut misplaced = inst(1);
        misplaced.id = "i-x".into();
        misplaced.status = Status::Paused;
        fs::write(
            dev.join("done/i-x.json"),
            serde_json::to_string(&misplaced).unwrap(),
        )
        .unwrap();
        assert!(s.instances_with("dev", Status::Paused).unwrap().is_empty());
        fs::remove_file(dev.join("done/i-x.json")).unwrap();

        // A ref's marker; one left by a failed write does not hold the ref.
        assert_eq!(
            fs::read_to_string(dev.join("refs/r1.json")).unwrap(),
            r#"{"R1":"i-a"}"#
        );
        fs::write(dev.join("refs/r9.json"), r#"{"R9":"i-a"}"#).unwrap();
        let mut c = inst(1);
        c.id = "i-d".into();
        c.r#ref = Some("R9".into());
        s.put_instance(&c).unwrap();
        assert_eq!(s.instance_by_ref("dev", "R9").unwrap().unwrap().id, "i-d");
        c.id = "i-e".into();
        assert_eq!(s.put_instance(&c), Err(HostError::Conflict));
        assert!(s.instance_by_ref("dev", "nope").unwrap().is_none());

        // A corrupt marker costs a scan, never an error; a write rebuilds it,
        // and `validate` hears of it (PLAN-006 D6-7).
        fs::write(dev.join("refs/r9.json"), "{ nope").unwrap();
        assert_eq!(s.unreadable_markers("dev").len(), 1);
        assert_eq!(s.instance_by_ref("dev", "R9").unwrap().unwrap().id, "i-d");
        let mut r10 = inst(1);
        r10.id = "i-r".into();
        r10.r#ref = Some("r9".into());
        s.put_instance(&r10).unwrap();
        assert!(s.unreadable_markers("dev").is_empty());
        assert_eq!(s.instance_by_ref("dev", "R9").unwrap().unwrap().id, "i-d");
        assert_eq!(s.instance_by_ref("dev", "r9").unwrap().unwrap().id, "i-r");

        // Refs that differ only in case share a marker, not an owner; a long
        // ref gets a short marker name.
        let long = "x/".repeat(150);
        for (id, r) in [
            ("i-f", "gh-1"),
            ("i-g", "GH-1"),
            ("i-h", long.as_str()),
            ("i-i", ""),
        ] {
            let mut i = inst(1);
            i.id = id.into();
            i.r#ref = Some(r.into());
            s.put_instance(&i).unwrap();
        }
        assert_eq!(s.instance_by_ref("dev", "gh-1").unwrap().unwrap().id, "i-f");
        assert_eq!(s.instance_by_ref("dev", "GH-1").unwrap().unwrap().id, "i-g");
        assert_eq!(s.instance_by_ref("dev", &long).unwrap().unwrap().id, "i-h");
        assert_eq!(s.instance_by_ref("dev", "").unwrap().unwrap().id, "i-i");
        let mut dup = inst(1);
        dup.id = "i-j".into();
        dup.r#ref = Some("GH-1".into());
        assert_eq!(s.put_instance(&dup), Err(HostError::Conflict));
        assert!(
            fs::read_dir(dev.join("refs"))
                .unwrap()
                .flatten()
                .all(|e| e.file_name().len() <= 125)
        );
        fs::remove_dir_all(d).ok();
    }
}
