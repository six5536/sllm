//! Idle: smllm's own state outside every machine. Its entry block is the idle
//! list; its events are `enter` and `resume` (IDLE, INST).
// @zen-component: IDLE-Idle

use crate::Error;
use crate::engine::machine::{commit, takeover_note};
use crate::engine::offer::{check_value, idle_offers};
use crate::engine::turn::Turn;
use crate::engine::{Location, Reply, api::random_id};
use crate::model::{ActionDef, Machine};
use crate::prelude::*;
use crate::record::{Instance, Status};
use crate::render::{Block, header, idle_header, quote};
use crate::utils::{SmallMap, insertion_sort_by_key};

/// The idle list (TURN-7): header, notes, `error:`, idle instructions, state
/// machines, suspended and parked instances, events.
// @zen-impl: TURN-7_AC-1
pub(crate) fn reply(
    turn: &mut Turn<'_, '_>,
    ok: bool,
    error: Option<String>,
) -> Result<Reply, Error> {
    let mut b = Block::open(&idle_header(&turn.session.key));
    list(turn, &mut b, error)?;
    Ok(Reply {
        ok,
        session: turn.session.key.clone(),
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
    let mut b = Block::open(&header(
        &turn.session.key,
        &machine.id,
        &inst.state,
        inst.visits(&inst.state),
        &machine.instance.kind,
        inst.label(),
    ));
    b.line(&format!("Arrived by: {arrived}"));
    if !turn.params.is_empty() {
        let ps: Vec<String> = turn
            .params
            .iter()
            .map(|(k, v)| format!("{k} = {}", quote(v)))
            .collect();
        b.line(&format!("Params: {}", ps.join(", ")));
    }
    let prompts = core::mem::take(&mut turn.prompts);
    b.lines(&turn.notes)
        .lines(&turn.trace)
        .lines(&turn.failures)
        .instructions(&prompts);
    turn.notes.clear();
    turn.trace.clear();
    turn.failures.clear();
    b.line(&format!(
        "Completed {} {}. You are now in idle.",
        machine.instance.kind,
        inst.label()
    ));
    list(turn, &mut b, None)?;
    Ok(Reply {
        ok: true,
        session: turn.session.key.clone(),
        location: Location::default(),
        text: b.close(),
    })
}

fn list(turn: &mut Turn<'_, '_>, b: &mut Block, error: Option<String>) -> Result<(), Error> {
    b.lines(&turn.notes)
        .lines(&turn.trace)
        .lines(&turn.failures);
    if let Some(e) = error {
        b.line(&format!("error: {e}"));
    }
    turn.prompts.clear();
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
        let eps: Vec<&str> = m.entry_points().map(|s| s.name.as_str()).collect();
        let mut starts = format!("    starts at: {}", m.initial);
        if !eps.is_empty() {
            starts.push_str(&format!("; entry points: {}", eps.join(", ")));
        }
        b.line(&starts);
    }

    let suspended = match &turn.session.suspended {
        Some(k) => match (
            turn.config.machine(&k.machine),
            turn.host.store.instance(&k.machine, &k.id)?,
        ) {
            (Some(m), Some(i))
                if i.status == Status::Suspended
                    && i.holder.as_deref() == Some(&turn.session.key) =>
            {
                Some((m, i))
            }
            _ => None,
        },
        None => None,
    };
    if let Some((m, i)) = &suspended {
        b.line(&format!(
            "Suspended: {} {} ({}) at {}",
            m.instance.kind,
            i.label(),
            m.id,
            i.state
        ));
    }
    let mut parked = Vec::new();
    for m in &turn.config.machines {
        for i in turn.host.store.instances(&m.id)? {
            if i.status == Status::Parked {
                parked.push((m, i));
            }
        }
    }
    insertion_sort_by_key(&mut parked, |(m, i)| (m.id.clone(), i.label().to_string()));
    if !parked.is_empty() {
        b.line("Parked:");
        for (m, i) in &parked {
            b.line(&format!(
                "- {} {} ({}) at {}",
                m.instance.kind,
                i.label(),
                m.id,
                i.state
            ));
        }
    }
    let offers = idle_offers(turn.config, suspended.as_ref().map(|(m, i)| (*m, i)));
    b.events(&turn.session.key, &offers);
    Ok(())
}

/// Fire `enter` or `resume` in idle.
pub(crate) fn fire(turn: &mut Turn<'_, '_>, params: &[(String, String)]) -> Result<Reply, Error> {
    match turn.event.as_str() {
        "enter" => enter(turn, params),
        "resume" if turn.session.suspended.is_some() => {
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
            let offered = if turn.session.suspended.is_some() {
                "enter, resume"
            } else {
                "enter"
            };
            let msg = format!("{} is not offered in idle (offered: {offered})", turn.event);
            reply(turn, false, Some(msg))
        }
    }
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
        let ids: Vec<&str> = config.machines.iter().map(|m| m.id.as_str()).collect();
        let msg = format!(
            "enter param stateMachine must be one of: {} (got \"{machine_id}\")",
            ids.join(", ")
        );
        return reply(turn, false, Some(msg));
    };
    let ref_param = machine.instance.ref_param.as_str();
    for (k, _) in params {
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
                    let eps: Vec<&str> = machine.entry_points().map(|s| s.name.as_str()).collect();
                    let msg = format!(
                        "{kind} {} is completed; to reopen it, fire enter with state: one of {}",
                        inst.label(),
                        if eps.is_empty() {
                            "(none: no state is an entry point)".to_string()
                        } else {
                            eps.join(", ")
                        }
                    );
                    return reply(turn, false, Some(msg));
                }
                (None, _, false) => {
                    let all: Vec<&str> = machine.states.iter().map(|s| s.name.as_str()).collect();
                    let msg = format!(
                        "saved state {} of {kind} {} no longer exists; fire enter with state: one of {}",
                        inst.state,
                        inst.label(),
                        all.join(", ")
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
        && matches!(inst.status, Status::Active | Status::Suspended)
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
    if turn.session.suspended.as_ref() == Some(&key) {
        turn.session.suspended = None;
    }
    inst.status = Status::Active;
    inst.holder = Some(turn.session.key.clone());
    // Entering the fallback state keeps its way back (IDLE-2).
    if !machine.state(target).is_some_and(|s| s.fallback) {
        inst.interrupted = None;
    }
    turn.session.holding = Some(key);
    turn.enter(machine, inst, target, from);
    turn.settle(machine, inst);
}

fn entry_point(machine: &Machine, state: &str) -> Result<(), String> {
    match machine.state(state) {
        Some(s) if s.entry_point => Ok(()),
        _ => {
            let eps: Vec<&str> = machine.entry_points().map(|s| s.name.as_str()).collect();
            Err(format!(
                "{state} is not an entry point of {} (entry points: {})",
                machine.id,
                if eps.is_empty() {
                    "none".to_string()
                } else {
                    eps.join(", ")
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
    Ok(turn
        .host
        .store
        .instances(&machine.id)?
        .into_iter()
        .find(|i| i.r#ref.as_deref() == Some(value)))
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
        interrupted: None,
        created: turn.now,
        updated: turn.now,
    })
}

/// `resume` from idle: back to the suspended instance's saved state.
fn resume(turn: &mut Turn<'_, '_>) -> Result<Reply, Error> {
    let key = turn.session.suspended.clone().expect("checked");
    let machine = turn.config.machine(&key.machine);
    let inst = turn.host.store.instance(&key.machine, &key.id)?;
    let (machine, mut inst) = match (machine, inst) {
        (Some(m), Some(i))
            if i.status == Status::Suspended && i.holder.as_deref() == Some(&turn.session.key) =>
        {
            (m, i)
        }
        (_, Some(i)) if i.holder.as_deref().is_some_and(|h| h != turn.session.key) => {
            turn.session.suspended = None;
            turn.host.store.put_session(&turn.session)?;
            let msg = format!(
                "{} moved to session {}",
                i.label(),
                i.holder.clone().unwrap_or_default()
            );
            return reply(turn, false, Some(msg));
        }
        _ => {
            turn.session.suspended = None;
            turn.host.store.put_session(&turn.session)?;
            return reply(
                turn,
                false,
                Some("the suspended instance is no longer suspended".to_string()),
            );
        }
    };
    if machine.state(&inst.state).is_none() {
        let all: Vec<&str> = machine.states.iter().map(|s| s.name.as_str()).collect();
        let msg = format!(
            "saved state {} no longer exists; fire enter with stateMachine {}, {} {}, state: one of {}",
            inst.state,
            machine.id,
            machine.instance.ref_param,
            inst.label(),
            all.join(", ")
        );
        return reply(turn, false, Some(msg));
    }
    let target = inst.state.clone();
    start(turn, machine, &mut inst, &target, Some(&target));
    commit(turn, machine, inst, Some(&target), "resume".to_string())
}
