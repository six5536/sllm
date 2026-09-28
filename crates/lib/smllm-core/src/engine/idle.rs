//! Idle: smllm's own state outside every machine. Its entry block is the idle
//! list; its events are `enter`, `resume` and `listPaused` (IDLE, INST).
// @zen-component: IDLE-Idle

use crate::Error;
use crate::engine::machine::{self, commit, owned, takeover_note};
use crate::engine::offer::{check_value, idle_offers};
use crate::engine::turn::Turn;
use crate::engine::{Location, Reply, api::random_id};
use crate::host::newest_first;
use crate::model::{ActionDef, Machine};
use crate::prelude::*;
use crate::record::{Instance, Status};
use crate::render::{Block, idle_header};
use crate::utils::SmallMap;
use crate::utils::join;

/// Paused instances the idle list shows; `listPaused` shows them all (IDLE-7).
pub(crate) const IDLE_PAUSED: usize = 10;

/// The idle list (TURN-7): header, notes, `error:`, idle instructions, state
/// machines, interrupted and paused instances, events.
// @zen-impl: TURN-7_AC-1
pub(crate) fn reply(
    turn: &mut Turn<'_, '_>,
    ok: bool,
    error: Option<String>,
) -> Result<Reply, Error> {
    // A rejected keyless call saved no session: show no key (TURN-3).
    let key = if turn.unsaved && !ok {
        String::new()
    } else {
        turn.session.key.clone()
    };
    let mut b = Block::open(&idle_header(&key));
    list(turn, &mut b, &key, error)?;
    Ok(Reply {
        ok,
        session: key,
        location: Location::default(),
        text: b.close(),
    })
}

/// A final state was entered: its block, then the idle list (IDLE-5).
pub(crate) fn after_final(
    turn: &mut Turn<'_, '_>,
    machine: &Machine,
    inst: &Instance,
    arrived: String,
) -> Result<Reply, Error> {
    let mut b = machine::head(turn, machine, inst, Some(&arrived));
    let prompts = core::mem::take(&mut turn.prompts);
    b.instructions(&prompts);
    turn.notes.clear();
    turn.trace.clear();
    turn.failures.clear();
    b.line(&format!(
        "Completed {} {}. You are now in idle.",
        machine.instance.kind,
        inst.label()
    ));
    let key = turn.session.key.clone();
    list(turn, &mut b, &key, None)?;
    Ok(Reply {
        ok: true,
        session: turn.session.key.clone(),
        location: Location::default(),
        text: b.close(),
    })
}

fn list(
    turn: &mut Turn<'_, '_>,
    b: &mut Block,
    key: &str,
    error: Option<String>,
) -> Result<(), Error> {
    b.lines(&turn.notes)
        .lines(&turn.trace)
        .lines(&turn.failures);
    if let Some(e) = error {
        b.line(&format!("error: {e}"));
    }
    // Prompts gathered so far (a paused or interrupted state's exit prompts)
    // come first, then idle's own.
    for a in &turn.config.idle {
        if let ActionDef::Prompt(p) = a {
            turn.prompt(p);
        }
    }
    let prompts = core::mem::take(&mut turn.prompts);
    b.instructions(&prompts);

    if turn.config.machines.is_empty() {
        b.line("No state machines are configured.");
    } else {
        b.line("State machines:");
    }
    for m in &turn.config.machines {
        match &m.description {
            Some(d) => b.line(&format!("- {} — {d}", m.id)),
            None => b.line(&format!("- {}", m.id)),
        };
        let mut id_line = format!("    {} id param: {}", m.instance.kind, m.instance.ref_param);
        if let Some(d) = &m.instance.ref_description {
            id_line.push_str(&format!(" — {d}"));
        }
        if let Some(p) = &m.instance.ref_pattern {
            id_line.push_str(&format!(" (pattern: {p})"));
        }
        b.line(&id_line);
        let eps = join(m.entry_point_names());
        let mut starts = format!("    starts at: {}", m.initial);
        if !eps.is_empty() {
            starts.push_str(&format!("; entry points: {}", eps));
        }
        b.line(&starts);
    }

    let interrupted = match &turn.session.interrupted {
        Some(k) => owned(
            turn.config,
            turn.host.store,
            k,
            Status::Interrupted,
            &turn.session.key,
        )?,
        None => None,
    };
    if let Some((m, i)) = &interrupted {
        b.line(&format!(
            "Interrupted: {} {} ({}) at {}",
            m.instance.kind,
            i.label(),
            m.id,
            i.state
        ));
    }
    // The newest few of each machine, merged: never every paused instance.
    let (paused, total) = paused(turn, Some(IDLE_PAUSED))?;
    paused_lines(b, &paused);
    if total > paused.len() {
        b.line(&format!(
            "…and {} more paused: fire listPaused to list them all.",
            total - paused.len()
        ));
    }
    let offers = idle_offers(
        turn.config,
        interrupted.as_ref().map(|(m, i)| (*m, i)),
        (total > IDLE_PAUSED).then_some(total),
    );
    b.events(key, &offers);
    Ok(())
}

/// Paused instances with their machines, in the idle list's order.
type Paused<'c> = Vec<(&'c Machine, Instance)>;

/// The paused instances of the configured machines, newest first (ties by
/// machine, then label), at most `limit` of them; and how many there are.
// @zen-impl: IDLE-7_AC-1
fn paused<'c>(turn: &mut Turn<'c, '_>, limit: Option<usize>) -> Result<(Paused<'c>, usize), Error> {
    let mut all = Vec::new();
    let mut total = 0;
    for m in &turn.config.machines {
        let store = &mut *turn.host.store;
        let n = store.count(&m.id, Status::Paused)?;
        total += n;
        all.extend(store.recent(&m.id, Status::Paused, limit.unwrap_or(n))?);
    }
    newest_first(&mut all);
    if let Some(n) = limit {
        all.truncate(n);
    }
    let machines = &turn.config.machines;
    let paused = all
        .into_iter()
        .filter_map(|i| Some((machines.iter().find(|m| m.id == i.machine)?, i)))
        .collect();
    Ok((paused, total))
}

fn paused_lines(b: &mut Block, paused: &[(&Machine, Instance)]) {
    if paused.is_empty() {
        return;
    }
    b.line("Paused:");
    for (m, i) in paused {
        b.line(&format!(
            "- {} {} ({}) at {}",
            m.instance.kind,
            i.label(),
            m.id,
            i.state
        ));
    }
}

/// `listPaused`: every paused instance, in the idle list's order, and the
/// idle events. A view: it writes nothing, not even the session, so it is
/// not an event for the stop rule (IDLE-7, PLAN-008 DC-1).
// @zen-impl: IDLE-7_AC-2
fn list_paused(turn: &mut Turn<'_, '_>) -> Result<Reply, Error> {
    let key = turn.session.key.clone();
    let (paused, total) = paused(turn, None)?;
    let interrupted = match &turn.session.interrupted {
        Some(k) => owned(turn.config, turn.host.store, k, Status::Interrupted, &key)?,
        None => None,
    };
    let mut b = Block::open(&idle_header(&key));
    paused_lines(&mut b, &paused);
    let offers = idle_offers(
        turn.config,
        interrupted.as_ref().map(|(m, i)| (*m, i)),
        (total > IDLE_PAUSED).then_some(total),
    );
    b.events(&key, &offers);
    Ok(Reply {
        ok: true,
        session: key,
        location: Location::default(),
        text: b.close(),
    })
}

/// Fire `enter`, `resume` or `listPaused` in idle.
pub(crate) fn fire(turn: &mut Turn<'_, '_>, params: &[(String, String)]) -> Result<Reply, Error> {
    let listable = turn.event == "listPaused" && more_paused(turn)?;
    match turn.event.as_str() {
        "enter" => enter(turn, params),
        "listPaused" if listable => {
            if let Some((n, _)) = params.first() {
                return reply(
                    turn,
                    false,
                    Some(format!("listPaused takes no params, but got {n}")),
                );
            }
            list_paused(turn)
        }
        "resume" if turn.session.interrupted.is_some() => {
            if let Some((n, _)) = params.first() {
                return reply(
                    turn,
                    false,
                    Some(format!("resume takes no params, but got {n}")),
                );
            }
            resume(turn)
        }
        _ => {
            let mut offered = vec!["enter"];
            if turn.session.interrupted.is_some() {
                offered.push("resume");
            }
            if more_paused(turn)? {
                offered.push("listPaused");
            }
            let msg = format!(
                "{} is not offered in idle (offered: {})",
                turn.event,
                join(offered)
            );
            reply(turn, false, Some(msg))
        }
    }
}

/// Whether the idle list is cut short, so `listPaused` is offered.
fn more_paused(turn: &mut Turn<'_, '_>) -> Result<bool, Error> {
    let mut total = 0;
    for m in &turn.config.machines {
        total += turn.host.store.count(&m.id, Status::Paused)?;
    }
    Ok(total > IDLE_PAUSED)
}

/// How `enter` reaches its state.
enum Arrival {
    New,
    Saved,
    Jump(String),
    Reopen(String),
    Repair(String),
}

/// `enter` (IDLE table, INST-3/6/10, IDLE-3/6).
// @zen-impl: INST-10_AC-1
// @zen-impl: IDLE-3_AC-1
// @zen-impl: IDLE-6_AC-1
fn enter(turn: &mut Turn<'_, '_>, params: &[(String, String)]) -> Result<Reply, Error> {
    let get = |n: &str| params.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone());
    let Some(machine_id) = get("stateMachine") else {
        return reply(
            turn,
            false,
            Some("enter needs param stateMachine".to_string()),
        );
    };
    let config = turn.config;
    let Some(machine) = config.machine(&machine_id) else {
        let ids = join(config.machines.iter().map(|m| m.id.as_str()));
        let msg = format!(
            "enter param stateMachine must be one of: {} (got \"{machine_id}\")",
            ids
        );
        return reply(turn, false, Some(msg));
    };
    let ref_param = machine.instance.ref_param.as_str();
    for (i, (k, _)) in params.iter().enumerate() {
        // One value per param: a repeat would reach the command environment
        // unchecked by the ref pattern (NFR-5).
        if params[..i].iter().any(|(p, _)| p == k) {
            return reply(
                turn,
                false,
                Some(format!("enter got param {k} more than once")),
            );
        }
        if k != "stateMachine" && k != "state" && k != ref_param {
            let msg = format!(
                "enter has no param {k} for {} (params: stateMachine, {ref_param}, state)",
                machine.id
            );
            return reply(turn, false, Some(msg));
        }
    }
    let kind = machine.instance.kind.as_str();
    let state_param = get("state");
    let id_value = get(ref_param);

    let existing = match &id_value {
        Some(v) => find(turn, machine, v)?,
        None => None,
    };
    let (mut inst, arrival) = match existing {
        None => {
            if let Some(v) = &id_value {
                let spec = crate::engine::ParamView {
                    name: ref_param.to_string(),
                    pattern: machine.instance.ref_pattern.clone(),
                    ..Default::default()
                };
                if let Err(e) = check_value("enter", &spec, v, turn.host.matcher) {
                    return reply(turn, false, Some(e));
                }
            }
            let start = match &state_param {
                Some(s) => {
                    if let Err(e) = entry_point(machine, s) {
                        return reply(turn, false, Some(e));
                    }
                    s.clone()
                }
                None => machine.initial.clone(),
            };
            (
                new_instance(turn, machine, id_value.clone(), start)?,
                Arrival::New,
            )
        }
        Some(inst) => {
            let saved_exists = machine.state(&inst.state).is_some();
            let arrival = match (&state_param, inst.status, saved_exists) {
                (None, Status::Completed, _) => {
                    let eps = join(machine.entry_point_names());
                    let msg = format!(
                        "{kind} {} is completed; to reopen it, fire enter with state: one of {}",
                        inst.label(),
                        if eps.is_empty() {
                            "(none: no state is an entry point)".to_string()
                        } else {
                            eps
                        }
                    );
                    return reply(turn, false, Some(msg));
                }
                (None, _, false) => {
                    let all = join(machine.state_names());
                    let msg = format!(
                        "saved state {} of {kind} {} no longer exists; fire enter with state: one of {}",
                        inst.state,
                        inst.label(),
                        all
                    );
                    return reply(turn, false, Some(msg));
                }
                (None, _, true) => Arrival::Saved,
                (Some(s), _, false) => {
                    if machine.state(s).is_none() {
                        return reply(
                            turn,
                            false,
                            Some(format!("{} has no state {s}", machine.id)),
                        );
                    }
                    Arrival::Repair(inst.state.clone())
                }
                (Some(s), status, true) => {
                    if let Err(e) = entry_point(machine, s) {
                        return reply(turn, false, Some(e));
                    }
                    if status == Status::Completed {
                        Arrival::Reopen(inst.state.clone())
                    } else {
                        Arrival::Jump(inst.state.clone())
                    }
                }
            };
            (inst, arrival)
        }
    };

    if let Some(h) = inst.holder.clone()
        && h != turn.session.key
        && matches!(inst.status, Status::Active | Status::Interrupted)
    {
        let note = takeover_note(turn, &h)?;
        turn.notes.push(note);
    }
    let target = state_param.clone().unwrap_or_else(|| inst.state.clone());
    let (from, arrived) = match &arrival {
        Arrival::New => (None, format!("enter (new {kind})")),
        Arrival::Saved => (Some(inst.state.clone()), "enter".to_string()),
        Arrival::Jump(f) => (Some(f.clone()), format!("enter (jump from {f})")),
        Arrival::Reopen(f) => {
            turn.notes
                .push(format!("Reopened {kind} {} at {target}.", inst.label()));
            (Some(f.clone()), format!("enter (reopened from {f})"))
        }
        Arrival::Repair(f) => (
            Some(f.clone()),
            format!("enter (saved state {f} no longer exists)"),
        ),
    };
    turn.params = params.to_vec();
    start(turn, machine, &mut inst, &target, from.as_deref());
    commit(turn, machine, inst, from.as_deref(), arrived)
}

/// Make `inst` active in this session and enter `target` (ACT-5).
// @zen-impl: ACT-5_AC-1
fn start(
    turn: &mut Turn<'_, '_>,
    machine: &Machine,
    inst: &mut Instance,
    target: &str,
    from: Option<&str>,
) {
    let key = inst.key();
    if turn.session.interrupted.as_ref() == Some(&key) {
        turn.session.interrupted = None;
    }
    inst.status = Status::Active;
    inst.holder = Some(turn.session.key.clone());
    // Entering the fallback state keeps its way back (IDLE-2).
    if !machine.state(target).is_some_and(|s| s.fallback) {
        inst.resume_state = None;
    }
    turn.session.holding = Some(key);
    turn.enter(machine, inst, target, from);
    turn.settle(machine, inst);
}

fn entry_point(machine: &Machine, state: &str) -> Result<(), String> {
    match machine.state(state) {
        Some(s) if s.entry_point => Ok(()),
        _ => {
            let eps = join(machine.entry_point_names());
            Err(format!(
                "{state} is not an entry point of {} (entry points: {})",
                machine.id,
                if eps.is_empty() {
                    "none".to_string()
                } else {
                    eps
                }
            ))
        }
    }
}

/// An instance by generated id or ref (INST-4).
// @zen-impl: INST-4_AC-1
fn find(
    turn: &mut Turn<'_, '_>,
    machine: &Machine,
    value: &str,
) -> Result<Option<Instance>, Error> {
    if let Some(i) = turn.host.store.instance(&machine.id, value)? {
        return Ok(Some(i));
    }
    Ok(turn.host.store.instance_by_ref(&machine.id, value)?)
}

/// A new instance with a fresh generated id (INST-2).
// @zen-impl: INST-2_AC-1
fn new_instance(
    turn: &mut Turn<'_, '_>,
    machine: &Machine,
    r#ref: Option<String>,
    state: String,
) -> Result<Instance, Error> {
    let id = loop {
        let id = format!("i-{}", random_id(turn.host, 6));
        if turn.host.store.instance(&machine.id, &id)?.is_none() {
            break id;
        }
    };
    Ok(Instance {
        id,
        machine: machine.id.clone(),
        r#ref,
        state,
        status: Status::Active,
        holder: None,
        version: 0,
        visits: SmallMap::new(),
        resume_state: None,
        created: turn.now,
        updated: turn.now,
    })
}

/// `resume` from idle: back to the interrupted instance's saved state.
fn resume(turn: &mut Turn<'_, '_>) -> Result<Reply, Error> {
    let key = turn.session.interrupted.clone().expect("checked");
    let machine = turn.config.machine(&key.machine);
    let inst = turn.host.store.instance(&key.machine, &key.id)?;
    let (machine, mut inst) = match (machine, inst) {
        (Some(m), Some(i)) if i.held_by(&turn.session.key, Status::Interrupted) => (m, i),
        // A briefly invalid machine file must not lose the interrupted
        // instance: report, change nothing.
        (None, Some(i)) if i.held_by(&turn.session.key, Status::Interrupted) => {
            let msg = format!(
                "state machine {} is not configured (is its file valid?); fix the config, then resume",
                key.machine
            );
            return reply(turn, false, Some(msg));
        }
        (_, Some(i)) if i.holder.as_deref().is_some_and(|h| h != turn.session.key) => {
            turn.session.interrupted = None;
            turn.host.store.put_session(&turn.session)?;
            let msg = format!(
                "{} moved to session {}",
                i.label(),
                i.holder.clone().unwrap_or_default()
            );
            return reply(turn, false, Some(msg));
        }
        _ => {
            turn.session.interrupted = None;
            turn.host.store.put_session(&turn.session)?;
            return reply(
                turn,
                false,
                Some("the interrupted instance is no longer interrupted".to_string()),
            );
        }
    };
    if machine.state(&inst.state).is_none() {
        let all = join(machine.state_names());
        let msg = format!(
            "saved state {} no longer exists; fire enter with stateMachine {}, {} {}, state: one of {}",
            inst.state,
            machine.id,
            machine.instance.ref_param,
            inst.label(),
            all
        );
        return reply(turn, false, Some(msg));
    }
    let target = inst.state.clone();
    start(turn, machine, &mut inst, &target, Some(&target));
    commit(turn, machine, inst, Some(&target), "resume".to_string())
}
