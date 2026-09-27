//! Wiring: configs → engine, file store and CLI host (STO-2, HOST-5).
// @zen-component: HOST-Runtime

use std::collections::HashMap;
use std::path::Path;

use smllm_core::host::{Host, Store};
use smllm_core::record::Session;
use smllm_core::{Bind, Engine, Reply};
use smllm_format::{ConfigFile, Findings, Loaded, MachineSource, Mode, load_configs};

use crate::error::Result;
use crate::host::{Commands, Files, OsIds};
use crate::paths;
use crate::store::FsStore;

/// A loaded config, its engine and the store.
pub struct Runtime {
    /// Where each loaded machine came from.
    pub sources: Vec<MachineSource>,
    /// Everything the load found.
    pub findings: Findings,
    /// The engine.
    pub engine: Engine,
    /// The store.
    pub store: FsStore,
    /// Config paths, recorded on new sessions.
    pub configs: Vec<String>,
}

impl Runtime {
    /// Load `files` to run them.
    pub fn new(files: &[ConfigFile]) -> Result<Self> {
        Self::load(files, Mode::Run)
    }

    /// Load `files` with every check, for `validate`.
    pub fn checked(files: &[ConfigFile]) -> Result<Self> {
        Self::load(files, Mode::Check)
    }

    fn load(files: &[ConfigFile], mode: Mode) -> Result<Self> {
        let Loaded {
            config,
            machines: sources,
            findings,
        } = load_configs(files, mode);
        let machines: HashMap<String, std::path::PathBuf> = sources
            .iter()
            .map(|m| (m.id.clone(), m.state_dir.clone()))
            .collect();
        let store = FsStore::new(paths::user_state_dir()?, machines);
        Ok(Self {
            engine: Engine::new(config),
            sources,
            findings,
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
        let session = FsStore::user()?.session(key).ok().flatten();
        Self::bound(session.as_ref())
    }

    /// The configs `session` was bound to; none without a session.
    pub fn bound(session: Option<&Session>) -> Result<Self> {
        Self::new(&paths::recorded(session.map_or(&[], |s| &s.configs[..])))
    }

    /// The runtime of a call: the configs recorded on session `key`, or for
    /// a keyless call those found from `cwd` (or `--config`).
    pub fn for_call(key: Option<&str>, explicit: Option<&Path>, cwd: &Path) -> Result<Self> {
        match key {
            Some(k) => Self::for_session(k),
            None => Self::lookup(explicit, cwd),
        }
    }

    /// Fire `event` for session `key`; with no key, an `enter` binds a new
    /// session to `harness` at `cwd` (HOST-3).
    pub fn fire(
        &mut self,
        harness: &str,
        key: Option<&str>,
        event: &str,
        params: &[(String, String)],
        cwd: &Path,
    ) -> Result<Reply> {
        let cwd = cwd.display().to_string();
        let configs = self.configs.clone();
        let bind = Bind {
            harness,
            host_session: None,
            cwd: &cwd,
            configs: &configs,
        };
        Ok(self.with(|e, h| e.fire(h, key, event, params, &bind))?)
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
