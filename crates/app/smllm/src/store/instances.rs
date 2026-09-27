//! Instances on disk (PLAN-005): live ones (active, suspended, parked) in
//! `<machine>/open/`, completed ones in `<machine>/done/`, and each ref as a
//! marker file `<machine>/refs/<ref>` holding its instance's id. The layout is
//! the index: status lines and idle lists read `open/` only, and a ref is one
//! file, however much history `done/` keeps (INST-9).
// @zen-component: STO-FileStore

use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

use agent_harness_kit::fs as fs_kit;
use smllm_core::host::HostError;
use smllm_core::record::{Instance, Status};

use super::{other, read_json, safe, write};

const OPEN: &str = "open";
const DONE: &str = "done";
const REFS: &str = "refs";

/// The directory an instance with `status` lives in.
fn shelf(status: Status) -> &'static str {
    if status == Status::Completed {
        DONE
    } else {
        OPEN
    }
}

fn file_name(id: &str) -> String {
    format!("{}.json", safe(id))
}

/// Run `f` holding the machine's lock (`<machine>/.lock`).
pub(super) fn locked<T>(
    dir: &Path,
    f: impl FnOnce() -> Result<T, HostError>,
) -> Result<T, HostError> {
    fs::create_dir_all(dir).map_err(other)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(".lock"))
        .map_err(other)?;
    lock.lock().map_err(other)?;
    let result = f();
    let _ = lock.unlock();
    result
}

/// Bring an older layout (`<machine>/<id>.json`) to the shelves, once. The
/// fast path is one stat: `open/` exists only once a migration is complete.
pub(super) fn migrate(dir: &Path) -> Result<(), HostError> {
    if dir.join(OPEN).is_dir() || !dir.is_dir() {
        return Ok(());
    }
    locked(dir, || migrate_locked(dir))
}

/// [`migrate`] under the lock. Files move to `done/` or to `open.new/`,
/// which becomes `open/` last, so a crash part-way resumes next time. A
/// file that does not parse goes to `open/`, where `validate` reports it.
fn migrate_locked(dir: &Path) -> Result<(), HostError> {
    let open = dir.join(OPEN);
    if open.is_dir() {
        return Ok(());
    }
    let staging = dir.join("open.new");
    for d in [&staging, &dir.join(DONE), &dir.join(REFS)] {
        fs::create_dir_all(d).map_err(other)?;
    }
    for e in fs::read_dir(dir).map_err(other)?.flatten() {
        let p = e.path();
        let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".json") || !p.is_file() {
            continue;
        }
        let inst = read_json::<Instance>(&p).ok().flatten();
        if let Some(Instance {
            id, r#ref: Some(r), ..
        }) = &inst
        {
            write(&dir.join(REFS).join(safe(r)), id)?;
        }
        let to = match &inst {
            Some(i) if i.status == Status::Completed => dir.join(DONE),
            _ => staging.clone(),
        };
        fs::rename(&p, to.join(name)).map_err(other)?;
    }
    fs::rename(&staging, &open).map_err(other)
}

/// Instance `id` and where it is: `open/` first, where most reads find it.
pub(super) fn find(dir: &Path, id: &str) -> Result<Option<(PathBuf, Instance)>, HostError> {
    for s in [OPEN, DONE] {
        let p = dir.join(s).join(file_name(id));
        if let Some(i) = read_json(&p)? {
            return Ok(Some((p, i)));
        }
    }
    Ok(None)
}

/// The instance whose ref is `r#ref`: its marker names it, and it has the
/// ref (a marker left by a failed write names one that does not).
pub(super) fn by_ref(dir: &Path, r#ref: &str) -> Result<Option<Instance>, HostError> {
    let Some(id) = fs_kit::read_text(&dir.join(REFS).join(safe(r#ref))).map_err(other)? else {
        return Ok(None);
    };
    Ok(find(dir, id.trim())?
        .map(|(_, i)| i)
        .filter(|i| i.r#ref.as_deref() == Some(r#ref)))
}

/// The readable instances of one shelf, and the unreadable files with why.
fn scan_shelf(dir: &Path, out: &mut (Vec<Instance>, Vec<(PathBuf, String)>)) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if !p.extension().is_some_and(|x| x == "json") {
            continue;
        }
        match read_json::<Instance>(&p) {
            Ok(Some(i)) => out.0.push(i),
            Ok(None) => {}
            Err(e) => out.1.push((p, e.to_string())),
        }
    }
}

/// Every instance file of both shelves (`None`), or of the shelf that holds
/// `status`, sorted by id.
pub(super) fn scan(dir: &Path, status: Option<Status>) -> (Vec<Instance>, Vec<(PathBuf, String)>) {
    let mut out = (Vec::new(), Vec::new());
    if migrate(dir).is_err() {
        return out;
    }
    match status {
        Some(s) => {
            scan_shelf(&dir.join(shelf(s)), &mut out);
            out.0.retain(|i| i.status == s);
        }
        None => {
            scan_shelf(&dir.join(OPEN), &mut out);
            scan_shelf(&dir.join(DONE), &mut out);
        }
    }
    out.0.sort_by(|a, b| a.id.cmp(&b.id));
    out.1.sort();
    out
}

/// Save `instance` (INST-8, INST-3): the version follows the stored one; a
/// new ref is free (no other instance has it as its ref or its id) and gets
/// its marker; the file is written where it is, then renamed to its shelf,
/// so it is never missing and never in both.
// @zen-impl: INST-8_AC-2
// @zen-impl: INST-3_AC-1
pub(super) fn put(dir: &Path, instance: &Instance) -> Result<(), HostError> {
    locked(dir, || {
        migrate_locked(dir)?;
        let stored = find(dir, &instance.id)?;
        let version = stored.as_ref().map_or(0, |(_, i)| i.version);
        if instance.version != version + 1 {
            return Err(HostError::Conflict);
        }
        let stored_ref = stored.as_ref().and_then(|(_, i)| i.r#ref.as_deref());
        if let Some(r) = instance.r#ref.as_deref()
            && stored_ref != Some(r)
        {
            let another = |i: Option<Instance>| i.is_some_and(|i| i.id != instance.id);
            if another(by_ref(dir, r)?) || another(find(dir, r)?.map(|(_, i)| i)) {
                return Err(HostError::Conflict);
            }
            write(&dir.join(REFS).join(safe(r)), &instance.id)?;
        }
        let text = serde_json::to_string_pretty(instance).map_err(other)?;
        let target = dir
            .join(shelf(instance.status))
            .join(file_name(&instance.id));
        match stored {
            Some((from, _)) if from != target => {
                write(&from, &text)?;
                fs::rename(&from, &target).map_err(other)
            }
            _ => write(&target, &text),
        }
    })
}
