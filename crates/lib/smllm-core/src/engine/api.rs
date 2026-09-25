//! The engine's public protocol: `bind`, `view`, `menu`, `fire`, `stop`,
//! `prompt_submitted` (HOST-1).
// @zen-component: ENG-Engine

#[cfg(feature = "serde")]
use serde::Serialize;

use crate::engine::turn::Turn;
use crate::engine::{idle, machine};
use crate::host::{Action, Guard, Host, HostError};
use crate::model::{ActionDef, Config, GuardDef, Transition};
use crate::prelude::*;
use crate::record::Session;

/// Crockford base32, lower-case: no i, l, o, u.
const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// An engine failure: not a rejected event (that is a [`Reply`] with
/// `ok: false`) but a call that could not be served.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// No session has this key (HOST-3).
    UnknownSession(String),
    /// No key given, and the event cannot bind one (HOST-3).
    MissingSession,
    /// The store failed.
    Host(HostError),
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::UnknownSession(k) => write!(
                f,
                "no smllm session {k}: pass the key shown in the latest <smllm> header, or call \
                 with no session and event enter to start a new one"
            ),
            Error::MissingSession => f.write_str(
                "no session given: pass the key shown in the latest <smllm> header, or fire \
                 enter with no session to start a new one",
            ),
            Error::Host(e) => write!(f, "store: {e}"),
        }
    }
}

#[cfg(feature = "std")]
extern crate std;
#[cfg(feature = "std")]
impl std::error::Error for Error {}

impl From<HostError> for Error {
    fn from(e: HostError) -> Self {
        Error::Host(e)
    }
}

/// Where a session is after a call.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize), serde(rename_all = "camelCase"))]
pub struct Location {
    /// State machine, or `None` in idle.
    pub machine: Option<String>,
    /// State.
    pub state: Option<String>,
    /// Instance id.
    pub instance: Option<String>,
    /// Instance ref.
    #[cfg_attr(feature = "serde", serde(rename = "ref"))]
    pub r#ref: Option<String>,
}

/// The answer to a call: the agent text, plus where the session ended up.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize), serde(rename_all = "camelCase"))]
pub struct Reply {
    /// False when the event was rejected or the instance moved (exit 1).
    pub ok: bool,
    /// The session key.
    pub session: String,
    /// Where the session is now.
    pub location: Location,
    /// The `<smllm>` text for the agent.
    pub text: String,
}

/// The stop hook's decision (TURN-4..6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// Let the turn end.
    Allow,
    /// Block with this events list.
    Block(String),
    /// Already continuing because of a stop hook and still no event: let it
    /// end, show the list elsewhere (TURN-6).
    Runaway(String),
}

/// How a session is bound (HOST-1).
#[derive(Debug, Clone, Default)]
pub struct Bind<'s> {
    /// Harness name (`claude`, `none`).
    pub harness: &'s str,
    /// The harness's session id, when it has one.
    pub host_session: Option<&'s str>,
    /// Working directory.
    pub cwd: &'s str,
    /// Config files, recorded on the session (STO-2).
    pub configs: &'s [String],
}

/// The engine: a config plus the protocol over a [`Host`].
#[derive(Debug, Clone)]
pub struct Engine {
    config: Config,
}

impl Engine {
    /// An engine for `config`.
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// The config.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Guard and action kinds this host cannot run (NFR-9, ACT-6).
    // @zen-impl: NFR-9_AC-1
    // @zen-impl: ACT-6_AC-1
    pub fn unsupported(&self, guards: &dyn Guard, actions: &dyn Action) -> Vec<String> {
        let mut out = Vec::new();
        let mut note = |m: &str, s: &str, what: &str, kind: &str| {
            let line = format!("{m}.{s}: {what} type {kind} is not supported by this host");
            if !out.contains(&line) {
                out.push(line);
            }
        };
        let action_list =
            |list: &[ActionDef], m: &str, s: &str, note: &mut dyn FnMut(&str, &str, &str, &str)| {
                for a in list {
                    if let ActionDef::Host { kind, .. } = a
                        && !actions.supports(kind)
                    {
                        note(m, s, "action", kind);
                    }
                }
            };
        let transition =
            |t: &Transition, m: &str, s: &str, note: &mut dyn FnMut(&str, &str, &str, &str)| {
                if let Some(GuardDef::Host { kind, .. }) = &t.guard
                    && !guards.supports(kind)
                {
                    note(m, s, "guard", kind);
                }
                action_list(&t.actions, m, s, note);
            };
        for m in &self.config.machines {
            for sh in &m.shared {
                action_list(&sh.entry, &m.id, "sharedActions", &mut note);
                action_list(&sh.exit, &m.id, "sharedActions", &mut note);
            }
            for s in &m.states {
                action_list(&s.entry, &m.id, &s.name, &mut note);
                action_list(&s.exit, &m.id, &s.name, &mut note);
                for t in s.on.iter().flat_map(|o| &o.transitions).chain(&s.always) {
                    transition(t, &m.id, &s.name, &mut note);
                }
            }
        }
        out
    }

    /// Bind a harness session: reuse its key, or create a session in idle.
    /// Returns the current entry block (session start / resume / compact).
    // @zen-impl: HOST-4_AC-1
    pub fn bind(&self, host: &mut Host<'_>, bind: &Bind<'_>) -> Result<Reply, Error> {
        if let Some(hs) = bind.host_session
            && let Some(key) = host.store.binding(bind.harness, hs)?
            && host.store.session(&key)?.is_some()
        {
            return self.view(host, &key);
        }
        let session = self.new_session(host, bind)?;
        if let Some(hs) = bind.host_session {
            host.store.put_binding(bind.harness, hs, &session.key)?;
        }
        let key = session.key.clone();
        host.store.put_session(&session)?;
        self.view(host, &key)
    }

    fn new_session(&self, host: &mut Host<'_>, bind: &Bind<'_>) -> Result<Session, Error> {
        let key = loop {
            let k = format!("sm-{}", random_id(host, 6));
            if host.store.session(&k)?.is_none() {
                break k;
            }
        };
        let now = host.clock.now_ms();
        Ok(Session {
            key,
            harness: bind.harness.to_string(),
            host_session: bind.host_session.map(ToString::to_string),
            cwd: bind.cwd.to_string(),
            configs: bind.configs.to_vec(),
            created: now,
            last_active: now,
            ..Session::default()
        })
    }

    fn session(&self, host: &mut Host<'_>, key: &str) -> Result<Session, Error> {
        host.store
            .session(key)?
            .ok_or_else(|| Error::UnknownSession(key.to_string()))
    }

    /// Read-only "where am I": entry block + events list; changes nothing (ENG-5).
    // @zen-impl: ENG-5_AC-1
    pub fn view(&self, host: &mut Host<'_>, key: &str) -> Result<Reply, Error> {
        let session = self.session(host, key)?;
        let mut turn = Turn::new(&self.config, host, session, "");
        if turn.session.holding.is_some() {
            return machine::view(&mut turn, true);
        }
        idle::reply(&mut turn, true, None)
    }

    /// The events list for the current state (the stop hook's text).
    pub fn menu(&self, host: &mut Host<'_>, key: &str) -> Result<Reply, Error> {
        let session = self.session(host, key)?;
        let mut turn = Turn::new(&self.config, host, session, "");
        if turn.session.holding.is_some() {
            return machine::view(&mut turn, false);
        }
        idle::reply(&mut turn, true, None)
    }

    /// Fire `event` with `params` (TURN-3). With no key, only `enter` may bind
    /// a new session, for harnesses without hooks (HOST-3).
    // @zen-impl: HOST-3_AC-1
    pub fn fire(
        &self,
        host: &mut Host<'_>,
        key: Option<&str>,
        event: &str,
        params: &[(String, String)],
        bind: &Bind<'_>,
    ) -> Result<Reply, Error> {
        let session = match key {
            Some(k) => self.session(host, k)?,
            // Saved only if the enter succeeds: a rejected keyless call
            // leaves nothing behind (TURN-3).
            None if event == "enter" => self.new_session(host, bind)?,
            None => return Err(Error::MissingSession),
        };
        let mut turn = Turn::new(&self.config, host, session, event);
        turn.session.last_active = turn.now;
        if turn.session.holding.is_some() {
            machine::fire(&mut turn, params)
        } else {
            idle::fire(&mut turn, params)
        }
    }

    /// The stop hook (TURN-4..6): in a machine state, block with the events
    /// list unless the agent yielded; idle and unknown sessions may stop.
    // @zen-impl: TURN-4_AC-1
    // @zen-impl: TURN-5_AC-1
    // @zen-impl: TURN-6_AC-1
    pub fn stop(
        &self,
        host: &mut Host<'_>,
        key: &str,
        stop_hook_active: bool,
    ) -> Result<Stop, Error> {
        let Some(session) = host.store.session(key)? else {
            return Ok(Stop::Allow);
        };
        if session.yielded || session.holding.is_none() {
            return Ok(Stop::Allow);
        }
        let mut turn = Turn::new(&self.config, host, session, "");
        // A moved instance is reported once, here, and the session drops to
        // idle (INST-7); an unconfigured machine lets the agent stop.
        let text = match machine::held(&mut turn, true)? {
            Ok(_) => machine::view(&mut turn, false)?.text,
            Err(machine::Gone::Moved(r)) => r.text,
            Err(machine::Gone::Unconfigured(_)) => return Ok(Stop::Allow),
        };
        Ok(if stop_hook_active {
            Stop::Runaway(text)
        } else {
            Stop::Block(text)
        })
    }

    /// A user prompt arrived: clear the yielded flag; inject nothing (TURN-8).
    // @zen-impl: TURN-8_AC-1
    pub fn prompt_submitted(&self, host: &mut Host<'_>, key: &str) -> Result<(), Error> {
        if let Some(mut s) = host.store.session(key)?
            && s.yielded
        {
            s.yielded = false;
            host.store.put_session(&s)?;
        }
        Ok(())
    }
}

/// `n` characters from [`ALPHABET`].
pub(crate) fn random_id(host: &mut Host<'_>, n: usize) -> String {
    let mut bits = host.ids.random();
    let mut out = String::with_capacity(n);
    for _ in 0..n {
        out.push(ALPHABET[(bits & 31) as usize] as char);
        bits >>= 5;
    }
    out
}
