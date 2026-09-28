//! Instances on disk (PLAN-005): live ones (active, interrupted, paused) in
//! `<machine>/open/`, completed ones in `<machine>/done/`, each ref in a
//! marker file under `<machine>/refs/`, and each history in
//! `<machine>/history/<id>.jsonl`. The layout is the index: status lines and
//! idle lists read `open/` only, and a ref is one file, however much history
//! `done/` keeps (INST-9). Reads never write; a write creates the folders it
//! needs (PLAN-006 D6-1).
// @zen-component: STO-FileStore

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

use smllm_core::host::HostError;
use smllm_core::record::{Instance, Status};

use super::{other, read_json, safe, write};

const OPEN: &str = "open";
const DONE: &str = "done";
const REFS: &str = "refs";
const HISTORY: &str = "history";

/// Instance `id`'s history: `<machine>/history/<id>.jsonl`.
pub(super) fn history_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(HISTORY).join(format!("{}.jsonl", safe(id)))
}

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
fn locked<T>(dir: &Path, f: impl FnOnce() -> Result<T, HostError>) -> Result<T, HostError> {
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

/// Instance `id` and where it is: `open/` first, where most reads find it.
/// A reopen moves a file from `done/` back to `open/` without the reader's
/// lock, so `open/` is read again after `done/`. On a case-insensitive file
/// system `I-X.json` opens `i-x.json`: only the exact id counts.
pub(super) fn find(dir: &Path, id: &str) -> Result<Option<(PathBuf, Instance)>, HostError> {
    let name = file_name(id);
    // Too long for a file name: no stored id (they are generated and short),
    // but a ref looked up as an id can be.
    if name.len() > 255 {
        return Ok(None);
    }
    for s in [OPEN, DONE, OPEN] {
        let p = dir.join(s).join(&name);
        if let Some(i) = read_json::<Instance>(&p)?.filter(|i| i.id == id) {
            return Ok(Some((p, i)));
        }
    }
    Ok(None)
}

/// Longest ref marker name, before `.json`; longer ones are cut and end in
/// a hash.
const MAX_MARKER: usize = 120;

/// The marker file of `r#ref`: a bucket of `ref → id` entries, named by
/// the ref folded to lower case, so refs that differ only in case share a
/// bucket rather than silently one file on macOS and Windows. A name over
/// [`MAX_MARKER`] bytes is cut and suffixed with the ref's FNV-1a hash; the
/// empty ref's is `~`.
fn marker(dir: &Path, r#ref: &str) -> PathBuf {
    let mut name = safe(r#ref).to_ascii_lowercase();
    if name.is_empty() {
        // The empty ref (no pattern forbids it): `safe` never yields `~`.
        name.push('~');
    } else if name.len() > MAX_MARKER {
        let hash = r#ref.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        });
        name.truncate(MAX_MARKER - 17);
        name.push_str(&format!("~{hash:016x}"));
    }
    dir.join(REFS).join(format!("{name}.json"))
}

type Bucket = BTreeMap<String, String>;

/// Record that instance `id` has `r#ref`. An unreadable marker is rebuilt
/// from the instances first (PLAN-006 D6-7).
fn add_ref(dir: &Path, r#ref: &str, id: &str) -> Result<(), HostError> {
    let path = marker(dir, r#ref);
    let mut bucket: Bucket = match read_json(&path) {
        Ok(b) => b.unwrap_or_default(),
        Err(_) => scan(dir, None)
            .0
            .into_iter()
            .filter_map(|i| Some((i.r#ref?, i.id)))
            .filter(|(r, _)| marker(dir, r) == path)
            .collect(),
    };
    bucket.insert(r#ref.to_string(), id.to_string());
    write(&path, &serde_json::to_string(&bucket).map_err(other)?)
}

/// The instance whose ref is `r#ref`: its marker names it, and it has the
/// ref (a marker left by a failed write names one that does not). Markers
/// are derived from the instances, which hold their refs: an unreadable one
/// costs a scan, never an error (PLAN-006 D6-7).
pub(super) fn by_ref(dir: &Path, r#ref: &str) -> Result<Option<Instance>, HostError> {
    let bucket: Option<Bucket> = match read_json(&marker(dir, r#ref)) {
        Ok(b) => b,
        Err(_) => {
            return Ok(scan(dir, None)
                .0
                .into_iter()
                .find(|i| i.r#ref.as_deref() == Some(r#ref)));
        }
    };
    let Some(id) = bucket.as_ref().and_then(|b| b.get(r#ref)) else {
        return Ok(None);
    };
    Ok(find(dir, id)?
        .map(|(_, i)| i)
        .filter(|i| i.r#ref.as_deref() == Some(r#ref)))
}

/// Marker files that cannot be read, with why: `validate` reports them.
pub(super) fn unreadable_markers(dir: &Path) -> Vec<(PathBuf, String)> {
    let Ok(rd) = fs::read_dir(dir.join(REFS)) else {
        return Vec::new();
    };
    let mut out: Vec<(PathBuf, String)> = rd
        .flatten()
        .map(|e| e.path())
        .filter_map(|p| read_json::<Bucket>(&p).err().map(|e| (p, e.to_string())))
        .collect();
    out.sort();
    out
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
    // An instance moving shelves while they are read may be seen on both.
    out.0.dedup_by(|a, b| a.id == b.id);
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
            add_ref(dir, r, &instance.id)?;
        }
        let text = serde_json::to_string_pretty(instance).map_err(other)?;
        let target = dir
            .join(shelf(instance.status))
            .join(file_name(&instance.id));
        match stored {
            Some((from, _)) if from != target => {
                write(&from, &text)?;
                fs::create_dir_all(dir.join(shelf(instance.status))).map_err(other)?;
                fs::rename(&from, &target).map_err(other)
            }
            _ => write(&target, &text),
        }
    })
}
