//! Wiring: configs → engine, file store and CLI host (STO-2, HOST-5).
// @zen-component: HOST-Runtime

use std::collections::HashMap;
use std::path::Path;

use smllm_core::host::{Host, Store};
use smllm_core::{Bind, Engine, Reply};
use smllm_format::{ConfigFile, Loaded, load_configs};

use crate::error::Result;
use crate::host::{Commands, Files, OsIds};
use crate::paths;
use crate::store::FsStore;

/// A loaded config, its engine and the store.
pub struct Runtime {
    /// The loaded config (findings included).
    pub loaded: Loaded,
    /// The engine.
    pub engine: Engine,
    /// The store.
    pub store: FsStore,
    /// Config paths, recorded on new sessions.
    pub configs: Vec<String>,
}

impl Runtime {
    /// Load `files`.
    pub fn new(files: &[ConfigFile]) -> Result<Self> {
        let loaded = load_configs(files, false);
        let machines: HashMap<String, std::path::PathBuf> = loaded
            .machines
            .iter()
            .map(|m| (m.id.clone(), m.state_dir.clone()))
            .collect();
        let store = FsStore::new(paths::user_state_dir()?, machines);
        Ok(Self {
            engine: Engine::new(loaded.config.clone()),
            loaded,
            store,
            configs: files.iter().map(|f| f.path.display().to_string()).collect(),
        })
    }

    /// The configs found from `cwd` (or `--config`).
    pub fn lookup(explicit: Option<&Path>, cwd: &Path) -> Result<Self> {
        Self::new(&paths::lookup(explicit, cwd)?)
    }

    /// The configs bound to session `key` when it was created (STO-2); none
    /// when the session is unknown.
    pub fn for_session(key: &str) -> Result<Self> {
        let mut bare = FsStore::new(paths::user_state_dir()?, HashMap::new());
        let configs = bare
            .session(key)
            .ok()
            .flatten()
            .map(|s| s.configs)
            .unwrap_or_default();
        Self::new(&paths::recorded(&configs))
    }

    /// Run `f` with the engine and a CLI host.
    pub fn with<R>(&mut self, f: impl FnOnce(&Engine, &mut Host<'_>) -> R) -> R {
        let (mut guards, mut actions, mut ids) = (Commands, Commands, OsIds);
        let mut host = Host {
            store: &mut self.store,
            guards: &mut guards,
            actions: &mut actions,
            source: &Files,
            matcher: &Files,
            clock: &Files,
            ids: &mut ids,
        };
        f(&self.engine, &mut host)
    }

    /// Bind a harness session; the entry block of where it is.
    pub fn bind(&mut self, harness: &str, host_session: Option<&str>, cwd: &str) -> Result<Reply> {
        let configs = self.configs.clone();
        let b = Bind {
            harness,
            host_session,
            cwd,
            configs: &configs,
        };
        Ok(self.with(|e, h| e.bind(h, &b))?)
    }
}
