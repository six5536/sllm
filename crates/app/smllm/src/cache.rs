//! The MCP server's loaded configs, kept between calls (HOST-16, PLAN-008
//! D8-8). Each call checks the files a load depended on, the way git checks
//! its index ("racy git"): a file whose size or modification time changed is
//! read and compared; so is one whose time is not clearly older than the load,
//! since an edit within the same timestamp tick would look unchanged; one that
//! was missing is looked for again. Any difference rebuilds. Everything else
//! is trusted, so after a tick a call reads no config file.
// @zen-component: HOST-Mcp

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use smllm_format::ConfigFile;

use crate::error::Result;
use crate::runtime::Runtime;

/// How much older than the load a file's time must be to be trusted
/// unread: covers a file system clock that differs from the process's (a VM
/// or network share) as well as a coarse timestamp (PLAN-008 DC-3).
const RACY: Duration = Duration::from_secs(2);

/// What a load saw of one input file.
struct Seen {
    path: PathBuf,
    /// `None` when the file was missing.
    file: Option<(SystemTime, u64, Vec<u8>)>,
}

impl Seen {
    fn now(path: PathBuf) -> Self {
        let file = fs::metadata(&path)
            .ok()
            .and_then(|m| Some((m.modified().ok()?, m.len(), fs::read(&path).ok()?)));
        Self { path, file }
    }

    /// Whether the file is as the load saw it.
    fn unchanged(&mut self, built: SystemTime) -> bool {
        let meta = fs::metadata(&self.path)
            .ok()
            .and_then(|m| Some((m.modified().ok()?, m.len())));
        match (&mut self.file, meta) {
            (None, None) => true,
            (Some((time, len, bytes)), Some((t, l))) => {
                let racy = t + RACY >= built;
                if (t, l) == (*time, *len) && !racy {
                    return true;
                }
                // Changed stamp, or too recent to trust: compare the bytes.
                match fs::read(&self.path) {
                    Ok(now) if now == *bytes => {
                        (*time, *len) = (t, l);
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }
}

struct Entry {
    configs: Vec<PathBuf>,
    inputs: Vec<Seen>,
    built: SystemTime,
    runtime: Runtime,
}

/// Loaded runtimes by the config files they came from.
#[derive(Default)]
pub struct ConfigCache {
    entries: Vec<Entry>,
}

impl ConfigCache {
    /// The runtime for `files`: the cached one when none of its inputs
    /// changed, else a fresh load (which replaces it).
    pub fn runtime(&mut self, files: &[ConfigFile]) -> Result<&mut Runtime> {
        let configs: Vec<PathBuf> = files.iter().map(|f| f.path.clone()).collect();
        let at = self.entries.iter().position(|e| e.configs == configs);
        if let Some(i) = at {
            let e = &mut self.entries[i];
            let built = e.built;
            if e.inputs.iter_mut().all(|s| s.unchanged(built)) {
                return Ok(&mut self.entries[i].runtime);
            }
            self.entries.swap_remove(i);
        }
        // The time is taken before reading, so a write during the load is
        // never trusted unread later.
        let built = SystemTime::now();
        let runtime = Runtime::new(files)?;
        let mut inputs: Vec<Seen> = Vec::new();
        for p in &runtime.inputs {
            if !inputs.iter().any(|s| &s.path == p) {
                inputs.push(Seen::now(p.clone()));
            }
        }
        self.entries.push(Entry {
            configs,
            inputs,
            built,
            runtime,
        });
        let last = self.entries.len() - 1;
        Ok(&mut self.entries[last].runtime)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smllm_format::Origin;

    fn machine(id: &str, description: &str) -> String {
        format!(
            "id: {id}\ndescription: {description}\ninitial: A\nmeta: {{ smllm: 1 }}\nstates:\n  A: {{ type: final }}\n"
        )
    }

    /// `id: description` of each loaded machine.
    fn loaded(rt: &Runtime) -> Vec<String> {
        let machines = &rt.engine.config().machines;
        machines
            .iter()
            .map(|m| format!("{}: {}", m.id, m.description.as_deref().unwrap_or("")))
            .collect()
    }

    fn set_time(path: &std::path::Path, t: SystemTime) {
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }

    // @zen-test: HOST-16_AC-1
    #[test]
    fn a_changed_input_reloads_even_within_a_tick() {
        let d = crate::test_support::temp_dir("cache");
        let (config, m, x) = (
            d.join("config.toml"),
            d.join("m.smllm.yaml"),
            d.join("x.smllm.yaml"),
        );
        fs::write(&config, "[machines]\nfiles = [\"m.smllm.yaml\"]\n").unwrap();
        fs::write(&m, machine("m", "aaa")).unwrap();
        let files = [ConfigFile {
            path: config.clone(),
            origin: Origin::Explicit,
        }];
        let mut cache = ConfigCache::default();
        assert_eq!(loaded(cache.runtime(&files).unwrap()), ["m: aaa"]);
        // Same size, same timestamp (set back): only the bytes differ.
        let t = fs::metadata(&m).unwrap().modified().unwrap();
        fs::write(&m, machine("m", "bbb")).unwrap();
        set_time(&m, t);
        assert_eq!(loaded(cache.runtime(&files).unwrap()), ["m: bbb"]);
        // A missing machine file that appears is picked up.
        fs::write(
            &config,
            "[machines]\nfiles = [\"m.smllm.yaml\", \"x.smllm.yaml\"]\n",
        )
        .unwrap();
        assert_eq!(loaded(cache.runtime(&files).unwrap()), ["m: bbb"]);
        fs::write(&x, machine("x", "xxx")).unwrap();
        assert_eq!(loaded(cache.runtime(&files).unwrap()), ["m: bbb", "x: xxx"]);
        // Old, untouched files are trusted unread: the load is reused.
        let old = SystemTime::now() - Duration::from_secs(60);
        for f in [&config, &m, &x] {
            set_time(f, old);
        }
        let first = cache.runtime(&files).unwrap() as *const Runtime;
        let again = cache.runtime(&files).unwrap() as *const Runtime;
        assert_eq!(first, again);
        fs::remove_dir_all(d).ok();
    }
}
