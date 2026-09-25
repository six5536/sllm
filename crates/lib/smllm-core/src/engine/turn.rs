//! One engine call's working state, and the transition machinery: guards,
//! exit → actions → entry, `always` chains, visits (ENG-2, DEC, ACT).
// @zen-component: ENG-Turn

use crate::host::{Call, Host};
use crate::model::{ActionDef, Config, GuardDef, Machine, Position, Prompt, Transition, Value};
use crate::prelude::*;
use crate::record::{Instance, Session};
use crate::utils::SmallMap;

/// Longest `always` chain before the engine gives up (DEC-2).
pub(crate) const ALWAYS_CAP: usize = 32;

/// Working state for one call.
pub(crate) struct Turn<'a, 'h> {
    pub config: &'a Config,
    pub host: &'a mut Host<'h>,
    pub session: Session,
    pub now: u64,
    /// The event being fired.
    pub event: String,
    /// Its checked params.
    pub params: Vec<(String, String)>,
    /// Header lines after the header (takeover, reopen, …).
    pub notes: Vec<String>,
    /// Guard results and pass-throughs (DEC-8).
    pub trace: Vec<String>,
    /// `always` states passed through.
    pub passed: Vec<String>,
    /// Failed actions (ACT-3).
    pub failures: Vec<String>,
    /// Prompt text gathered in order (ACT-2).
    pub prompts: Vec<String>,
}

/// `issueId` → `ISSUE_ID`.
pub(crate) fn env_name(name: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for c in name.chars() {
        if c.is_ascii_uppercase() && prev_lower {
            out.push('_');
        }
        prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        out.push(if c.is_ascii_alphanumeric() {
            c.to_ascii_uppercase()
        } else {
            '_'
        });
    }
    out
}

/// Where an action list comes from, for failure messages.
pub(crate) struct ListLabel<'s> {
    pub what: &'s str,
    /// State for `SMLLM_STATE`.
    pub state: &'s str,
    pub from: Option<&'s str>,
    pub to: Option<&'s str>,
}

impl<'a, 'h> Turn<'a, 'h> {
    pub(crate) fn new(
        config: &'a Config,
        host: &'a mut Host<'h>,
        session: Session,
        event: &str,
    ) -> Self {
        let now = host.clock.now_ms();
        Self {
            config,
            host,
            session,
            now,
            event: event.to_string(),
            params: Vec::new(),
            notes: Vec::new(),
            trace: Vec::new(),
            passed: Vec::new(),
            failures: Vec::new(),
            prompts: Vec::new(),
        }
    }

    /// The command environment (DEC-6, ACT-4).
    // @zen-impl: DEC-6_AC-1
    pub(crate) fn env(
        &self,
        inst: &Instance,
        state: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Vec<(String, String)> {
        let mut env = vec![
            ("SMLLM_SESSION".to_string(), self.session.key.clone()),
            ("SMLLM_MACHINE".to_string(), inst.machine.clone()),
            ("SMLLM_STATE".to_string(), state.to_string()),
            ("SMLLM_EVENT".to_string(), self.event.clone()),
            ("SMLLM_INSTANCE".to_string(), inst.id.clone()),
            (
                "SMLLM_REF".to_string(),
                inst.r#ref.clone().unwrap_or_default(),
            ),
        ];
        if let Some(f) = from {
            env.push(("SMLLM_FROM".to_string(), f.to_string()));
        }
        if let Some(t) = to {
            env.push(("SMLLM_TO".to_string(), t.to_string()));
        }
        for (k, v) in &self.params {
            env.push((format!("SMLLM_PARAM_{}", env_name(k)), v.clone()));
        }
        env
    }

    /// Evaluate a guard, recording the result in the trace (DEC-5, DEC-8).
    fn guard(&mut self, inst: &Instance, state: &str, guard: &GuardDef) -> bool {
        match guard {
            GuardDef::Visits { state: s, at_least } => {
                let n = inst.visits(s);
                let ok = n >= *at_least;
                self.trace.push(format!(
                    "Guard: visits {s} ≥ {at_least} → {ok} (visits: {n})"
                ));
                ok
            }
            GuardDef::Host { kind, params } => {
                let env = self.env(inst, state, None, None);
                let out = self.host.guards.check(&Call {
                    machine: &inst.machine,
                    kind,
                    params,
                    env: &env,
                    cwd: &self.session.cwd,
                });
                let mut line = format!("Guard: {} → {}", describe(kind, params), out.ok);
                if !out.detail.is_empty() {
                    line.push_str(&format!(" ({})", out.detail));
                }
                self.trace.push(line);
                out.ok
            }
        }
    }

    /// The first transition whose guard passes (DEC-1).
    // @zen-impl: DEC-1_AC-1
    pub(crate) fn pick<'t>(
        &mut self,
        inst: &Instance,
        state: &str,
        candidates: &'t [Transition],
    ) -> Option<&'t Transition> {
        for t in candidates {
            match &t.guard {
                None => return Some(t),
                Some(g) => {
                    if self.guard(inst, state, g) {
                        return Some(t);
                    }
                }
            }
        }
        None
    }

    /// Run one action list: commands now, prompts gathered; a failed command
    /// skips the rest of the list's commands (ACT-2, ACT-3).
    // @zen-impl: ACT-2_AC-1
    // @zen-impl: ACT-3_AC-1
    pub(crate) fn run(&mut self, inst: &mut Instance, list: &[ActionDef], label: &ListLabel<'_>) {
        let mut failed = false;
        for (i, action) in list.iter().enumerate() {
            match action {
                ActionDef::Prompt(p) => self.prompt(p),
                ActionDef::SetRef => {
                    // Checked before the transition was taken (INST-3).
                    let param = self
                        .config
                        .machine(&inst.machine)
                        .map(|m| m.instance.ref_param.clone())
                        .unwrap_or_default();
                    if let Some((_, v)) = self.params.iter().find(|(n, _)| *n == param) {
                        inst.r#ref = Some(v.clone());
                    }
                }
                ActionDef::Host { kind, params } => {
                    if failed {
                        continue;
                    }
                    let env = self.env(inst, label.state, label.from, label.to);
                    let out = self.host.actions.run(&Call {
                        machine: &inst.machine,
                        kind,
                        params,
                        env: &env,
                        cwd: &self.session.cwd,
                    });
                    if !out.ok {
                        failed = true;
                        self.failures.push(format!(
                            "Action failed: {}[{i}] {}: {}",
                            label.what, kind, out.detail
                        ));
                    }
                }
            }
        }
    }

    pub(crate) fn prompt(&mut self, prompt: &Prompt) {
        match prompt {
            Prompt::Text(t) => self.prompts.push(t.clone()),
            Prompt::File(f) | Prompt::DefaultFile(f) => match self.host.source.read(f) {
                Ok(Some(t)) => self.prompts.push(t),
                Ok(None) if matches!(prompt, Prompt::DefaultFile(_)) => {}
                Ok(None) => self.failures.push(format!("Prompt failed: {f}: not found")),
                Err(e) => self.failures.push(format!("Prompt failed: {f}: {e}")),
            },
        }
    }

    /// Enter `name`: count the visit, then shared-before, own, shared-after
    /// `entry` (ENG-3, CFG-11).
    // @zen-impl: ENG-3_AC-1
    // @zen-impl: CFG-11_AC-1
    pub(crate) fn enter(
        &mut self,
        machine: &Machine,
        inst: &mut Instance,
        name: &str,
        from: Option<&str>,
    ) {
        inst.state = name.to_string();
        let n = inst.visits(name) + 1;
        inst.visits.insert(name, n);
        let Some(state) = machine.state(name) else {
            return;
        };
        for (what, list) in lists(machine, name, &state.entry, true) {
            self.run(
                inst,
                list,
                &ListLabel {
                    what: &what,
                    state: name,
                    from,
                    to: Some(name),
                },
            );
        }
    }

    /// Leave `name`: shared-before, own, shared-after `exit`.
    pub(crate) fn exit(
        &mut self,
        machine: &Machine,
        inst: &mut Instance,
        name: &str,
        to: Option<&str>,
    ) {
        let Some(state) = machine.state(name) else {
            return;
        };
        for (what, list) in lists(machine, name, &state.exit, false) {
            self.run(
                inst,
                list,
                &ListLabel {
                    what: &what,
                    state: name,
                    from: Some(name),
                    to,
                },
            );
        }
    }

    /// Take a picked transition from `source` (ACT-1): external ones run
    /// exit → actions → entry; targetless and non-`reenter` self transitions
    /// only their actions (CFG-3).
    // @zen-impl: ACT-1_AC-1
    // @zen-impl: CFG-3_AC-1
    pub(crate) fn take(
        &mut self,
        machine: &Machine,
        inst: &mut Instance,
        source: &str,
        t: &Transition,
    ) {
        let target = t.target.as_deref();
        let external = match target {
            None => false,
            Some(tg) => tg != source || t.reenter,
        };
        let what = format!("{source} transition actions");
        let label = |what| ListLabel {
            what,
            state: source,
            from: Some(source),
            to: target,
        };
        if !external {
            self.run(inst, &t.actions, &label(&what));
            return;
        }
        let target = target.unwrap_or(source);
        self.exit(machine, inst, source, Some(target));
        self.run(inst, &t.actions, &label(&what));
        self.enter(machine, inst, target, Some(source));
    }

    /// Leave eventless states until one without `always` (DEC-2).
    // @zen-impl: DEC-2_AC-1
    pub(crate) fn settle(&mut self, machine: &Machine, inst: &mut Instance) {
        for _ in 0..ALWAYS_CAP {
            let current = inst.state.clone();
            let Some(state) = machine.state(&current) else {
                return;
            };
            if state.always.is_empty() {
                return;
            }
            let Some(t) = self.pick(inst, &current, &state.always) else {
                self.failures.push(format!(
                    "No always transition of {current} matched; stopped there"
                ));
                return;
            };
            self.passed.push(current.clone());
            self.take(machine, inst, &current, t);
        }
        self.failures.push(format!(
            "Stopped after {ALWAYS_CAP} always transitions; check for a loop"
        ));
    }
}

/// Shared-before lists, the state's own, shared-after, each labelled.
fn lists<'m>(
    machine: &'m Machine,
    state: &str,
    own: &'m [ActionDef],
    entry: bool,
) -> Vec<(String, &'m [ActionDef])> {
    let kind = if entry { "entry" } else { "exit" };
    let shared = |pos: Position| {
        machine
            .shared
            .iter()
            .enumerate()
            .filter(move |(_, s)| s.position == pos && s.states.iter().any(|n| n == state))
            .map(move |(i, s)| {
                let list: &[ActionDef] = if entry { &s.entry } else { &s.exit };
                (format!("sharedActions[{i}] {kind}"), list)
            })
    };
    let mut out: Vec<(String, &[ActionDef])> = shared(Position::Before).collect();
    out.push((format!("{state} {kind}"), own));
    out.extend(shared(Position::After));
    out
}

/// `command "cargo test"` for the trace.
fn describe(kind: &str, params: &SmallMap<Value>) -> String {
    match params.get("run") {
        Some(Value::Str(s)) => format!("{kind} `{s}`"),
        Some(Value::List(l)) => format!("{kind} [{}]", l.join(" ")),
        _ => kind.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_names_are_screaming_snake() {
        assert_eq!(env_name("issueId"), "ISSUE_ID");
        assert_eq!(env_name("summary"), "SUMMARY");
        assert_eq!(env_name("a-b2C"), "A_B2_C");
    }

    #[test]
    fn describe_shows_the_command() {
        let mut p = SmallMap::new();
        assert_eq!(describe("command", &p), "command");
        p.insert("run", Value::Str("x y".into()));
        assert_eq!(describe("command", &p), "command `x y`");
        p.insert("run", Value::List(vec!["x".into(), "y".into()]));
        assert_eq!(describe("command", &p), "command [x y]");
    }
}
