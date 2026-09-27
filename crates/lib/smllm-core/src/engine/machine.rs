//! Events fired inside a state machine: its own events and the built-ins
//! `yield`, `park`, `unmatched`, `resume` (from a fallback state).
// @zen-component: ENG-Machine

use crate::Error;
use crate::engine::offer::{check_params, machine_offers};
use crate::engine::turn::{Turn, lists};
use crate::engine::{Location, Reply, idle};
use crate::host::HostError;
use crate::model::{ActionDef, Machine};
use crate::prelude::*;
use crate::record::{HistoryEntry, Instance, Status};
use crate::render::{Block, format_utc, header, quote};

/// Why the held instance cannot be used.
pub(crate) enum Gone {
    /// Its machine is not in the (current) config: reported, never saved, so
    /// a briefly broken file does not lose the session's place.
    Unconfigured(Box<Reply>),
    /// Another session holds it, or it is no longer active or present (INST-7).
    Moved(Box<Reply>),
}

impl Gone {
    pub(crate) fn reply(self) -> Reply {
        match self {
            Gone::Unconfigured(r) | Gone::Moved(r) => *r,
        }
    }
}

/// An event while the held instance is gone. `enter` is idle's event, and
/// idle is where a gone instance leaves the session: after a move it goes
/// on from idle (the reply already said the session is idle). With the
/// machine unconfigured, `park` and `enter` let go of the instance (its saved
/// state is untouched) so the session is never stuck; anything else gets
/// the error reply.
fn when_gone(
    turn: &mut Turn<'_, '_>,
    gone: Gone,
    params: &[(String, String)],
) -> Result<Reply, Error> {
    let event = turn.event.as_str();
    match gone {
        Gone::Moved(_) if event == "enter" => idle::fire(turn, params),
        Gone::Unconfigured(_) if event == "park" || event == "enter" => {
            if let Some(key) = turn.session.holding.take() {
                turn.notes.push(format!(
                    "Let go of instance {} of the unconfigured state machine {}; its saved state is unchanged.",
                    key.id, key.machine
                ));
            }
            if event == "enter" {
                return idle::fire(turn, params);
            }
            turn.host.store.put_session(&turn.session)?;
            idle::reply(turn, true, None)
        }
        gone => Ok(gone.reply()),
    }
}

/// The held instance, or why not. `persist`: drop the session to idle in the
/// store when it moved (event calls and the stop hook); views never write
/// (ENG-5).
pub(crate) fn held<'c>(
    turn: &mut Turn<'c, '_>,
    persist: bool,
) -> Result<Result<(&'c Machine, Instance), Gone>, Error> {
    let key = turn
        .session
        .holding
        .clone()
        .expect("caller checked holding");
    let Some(machine) = turn.config.machine(&key.machine) else {
        let msg = format!(
            "state machine {} is not configured (is its file valid?); fix the config, or park",
            key.machine
        );
        return idle::reply(turn, false, Some(msg)).map(|r| Err(Gone::Unconfigured(Box::new(r))));
    };
    let inst = turn.host.store.instance(&key.machine, &key.id)?;
    match inst {
        Some(i) if i.holder.as_deref() == Some(&turn.session.key) && i.status == Status::Active => {
            Ok(Ok((machine, i)))
        }
        Some(i) => {
            let kind = &machine.instance.kind;
            let msg = match &i.holder {
                Some(h) if *h != turn.session.key => {
                    format!("{kind} {} moved to session {h}; you are in idle", i.label())
                }
                _ => format!(
                    "{kind} {} is {}; you are in idle",
                    i.label(),
                    i.status.as_str()
                ),
            };
            gone(turn, msg, persist)
        }
        None => gone(
            turn,
            format!("instance {} no longer exists; you are in idle", key.id),
            persist,
        ),
    }
}

fn gone(
    turn: &mut Turn<'_, '_>,
    msg: String,
    persist: bool,
) -> Result<Result<(&'static Machine, Instance), Gone>, Error> {
    let r = if persist {
        moved(turn, msg)?
    } else {
        idle::reply(turn, false, Some(msg))?
    };
    Ok(Err(Gone::Moved(Box::new(r))))
}

/// Drop to idle with an error (INST-7).
// @zen-impl: INST-7_AC-1
pub(crate) fn moved(turn: &mut Turn<'_, '_>, msg: String) -> Result<Reply, Error> {
    // Nothing was entered: drop the prompts this call gathered.
    turn.prompts.clear();
    turn.session.holding = None;
    turn.host.store.put_session(&turn.session)?;
    idle::reply(turn, false, Some(msg))
}

/// No event: entry block (optionally) + events list (ENG-5).
pub(crate) fn view(turn: &mut Turn<'_, '_>, entry: bool) -> Result<Reply, Error> {
    let (machine, inst) = match held(turn, false)? {
        Ok(p) => p,
        Err(gone) => return Ok(gone.reply()),
    };
    if entry && let Some(state) = machine.state(&inst.state) {
        // Re-render the state's entry prompts only; commands are not re-run.
        for (_, list) in lists(machine, &state.name, &state.entry, true) {
            for a in list {
                if let ActionDef::Prompt(p) = a {
                    turn.prompt(p);
                }
            }
        }
    }
    missing_state_note(turn, machine, &inst);
    Ok(block(turn, machine, &inst, None, entry, true, None))
}

fn missing_state_note(turn: &mut Turn<'_, '_>, machine: &Machine, inst: &Instance) {
    if machine.state(&inst.state).is_none() {
        let names: Vec<&str> = machine.states.iter().map(|s| s.name.as_str()).collect();
        turn.notes.push(format!(
            "Saved state {} no longer exists in {}. Fire park, then enter with a state: {}.",
            inst.state,
            machine.id,
            names.join(", ")
        ));
    }
}

/// A block opened with the header of where `inst` rests (TURN-1).
pub(crate) fn open(turn: &Turn<'_, '_>, machine: &Machine, inst: &Instance) -> Block {
    Block::open(&header(
        &turn.session.key,
        &machine.id,
        &inst.state,
        inst.visits(&inst.state),
        &machine.instance.kind,
        inst.label(),
    ))
}

/// [`open`], then `Arrived by`, `Params`, notes, trace and failures.
pub(crate) fn head(
    turn: &Turn<'_, '_>,
    machine: &Machine,
    inst: &Instance,
    arrived: Option<&str>,
) -> Block {
    let mut b = open(turn, machine, inst);
    if let Some(a) = arrived {
        b.line(&format!("Arrived by: {a}"));
    }
    if !turn.params.is_empty() {
        let ps: Vec<String> = turn
            .params
            .iter()
            .map(|(k, v)| format!("{k} = {}", quote(v)))
            .collect();
        b.line(&format!("Params: {}", ps.join(", ")));
    }
    b.lines(&turn.notes)
        .lines(&turn.trace)
        .lines(&turn.failures);
    b
}

/// Render a machine-state block; `ok` unless there is an `error`.
fn block(
    turn: &mut Turn<'_, '_>,
    machine: &Machine,
    inst: &Instance,
    arrived: Option<&str>,
    instructions: bool,
    events: bool,
    error: Option<String>,
) -> Reply {
    let mut b = head(turn, machine, inst, arrived);
    let ok = error.is_none();
    if let Some(e) = &error {
        b.line(&format!("error: {e}"));
    }
    if instructions {
        b.instructions(&turn.prompts);
    }
    if events {
        let offers = machine_offers(machine, machine.state(&inst.state), inst);
        b.events(&turn.session.key, &offers);
    }
    Reply {
        ok,
        session: turn.session.key.clone(),
        location: location(machine, inst),
        text: b.close(),
    }
}

pub(crate) fn location(machine: &Machine, inst: &Instance) -> Location {
    Location {
        machine: Some(machine.id.clone()),
        state: Some(inst.state.clone()),
        instance: Some(inst.id.clone()),
        r#ref: inst.r#ref.clone(),
    }
}

/// A rejected call: header, `error:`, events list; nothing changes (TURN-3).
fn reject(
    turn: &mut Turn<'_, '_>,
    machine: &Machine,
    inst: &Instance,
    msg: String,
) -> Result<Reply, Error> {
    turn.params.clear();
    missing_state_note(turn, machine, inst);
    Ok(block(turn, machine, inst, None, false, true, Some(msg)))
}

/// Fire an event in a machine state.
// @zen-impl: ENG-2_AC-1
pub(crate) fn fire(turn: &mut Turn<'_, '_>, params: &[(String, String)]) -> Result<Reply, Error> {
    let (machine, mut inst) = match held(turn, true)? {
        Ok(p) => p,
        Err(gone) => return when_gone(turn, gone, params),
    };
    let state = machine.state(&inst.state);
    let offers = machine_offers(machine, state, &inst);
    let Some(offer) = offers.iter().find(|o| o.name == turn.event) else {
        let names: Vec<&str> = offers.iter().map(|o| o.name.as_str()).collect();
        let msg = format!(
            "{} is not offered here (offered: {})",
            turn.event,
            names.join(", ")
        );
        return reject(turn, machine, &inst, msg);
    };
    match check_params(offer, params, turn.host.matcher) {
        Ok(p) => turn.params = p,
        Err(e) => return reject(turn, machine, &inst, e),
    }
    let from = inst.state.clone();
    let event = turn.event.clone();
    match event.as_str() {
        "yield" => {
            turn.session.yielded = true;
            inst.version += 1;
            inst.updated = turn.now;
            if let Err(r) = save(turn, machine, &mut inst, Some(&from), Some(&from))? {
                return Ok(r);
            }
            let mut b = open(turn, machine, &inst);
            b.line(&format!(
                "Yielded: staying in {from}. You may end your turn."
            ));
            Ok(Reply {
                ok: true,
                session: turn.session.key.clone(),
                location: location(machine, &inst),
                text: b.close(),
            })
        }
        "park" => {
            turn.exit(machine, &mut inst, &from, None);
            inst.status = Status::Parked;
            inst.holder = None;
            turn.session.holding = None;
            turn.notes.push(format!(
                "Parked {} {} at {from}.",
                machine.instance.kind,
                inst.label()
            ));
            leave_to_idle(turn, machine, inst, &from)
        }
        "unmatched" => unmatched(turn, machine, inst, &from),
        "resume" => {
            let back = inst.interrupted.take().unwrap_or_default();
            if machine.state(&back).is_none() {
                let all: Vec<&str> = machine.states.iter().map(|s| s.name.as_str()).collect();
                let msg = format!(
                    "the interrupted state {back} no longer exists; leave with one of this state's events, or park and enter with a state: {}",
                    all.join(", ")
                );
                inst.interrupted = Some(back);
                return reject(turn, machine, &inst, msg);
            }
            turn.exit(machine, &mut inst, &from, Some(&back));
            turn.enter(machine, &mut inst, &back, Some(&from));
            turn.settle(machine, &mut inst);
            commit(turn, machine, inst, Some(&from), "resume".to_string())
        }
        _ => {
            let on = state.and_then(|s| s.on(&event)).expect("offered");
            let Some(t) = turn.pick(&inst, &from, &on.transitions) else {
                let msg = format!("no transition of {event} matched in {from}; nothing changed");
                return reject(turn, machine, &inst, msg);
            };
            // Only the chosen transition's setRef counts; checked before any
            // action runs, so a rejected call changes nothing (TURN-3).
            if t.actions.contains(&ActionDef::SetRef)
                && let Err(msg) = check_set_ref(turn, machine, &inst)
            {
                return reject(turn, machine, &inst, msg);
            }
            // Leaving a fallback state ends the detour; a transition back
            // into it keeps its way back (`resume`).
            if machine.state(&inst.state).is_some_and(|s| s.fallback)
                && t.target.as_deref().is_some_and(|to| to != inst.state)
            {
                inst.interrupted = None;
            }
            turn.take(machine, &mut inst, &from, t);
            turn.settle(machine, &mut inst);
            commit(
                turn,
                machine,
                inst,
                Some(&from),
                format!("{event} from {from}"),
            )
        }
    }
}

/// `setRef` may run once, with a ref no other instance has; setting the ref
/// the instance already has changes nothing (INST-3).
// @zen-impl: INST-3_AC-1
fn check_set_ref(
    turn: &mut Turn<'_, '_>,
    machine: &Machine,
    inst: &Instance,
) -> Result<(), String> {
    let kind = &machine.instance.kind;
    let param = &machine.instance.ref_param;
    let Some((_, value)) = turn.params.iter().find(|(n, _)| n == param) else {
        return Err(format!("{} needs param {param}", turn.event));
    };
    match &inst.r#ref {
        Some(r) if r == value => return Ok(()),
        Some(r) => {
            return Err(format!(
                "{kind} {} already has its ref {r}; it is set once",
                inst.id
            ));
        }
        None => {}
    }
    let taken = turn
        .host
        .store
        .instances(&machine.id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .any(|i| i.id != inst.id && (i.r#ref.as_deref() == Some(value) || i.id == *value));
    if taken {
        return Err(format!(
            "another {kind} of {} already has ref {value}",
            machine.id
        ));
    }
    Ok(())
}

/// `unmatched`: to the fallback state, or suspend and go to idle (IDLE-2).
// @zen-impl: IDLE-2_AC-1
fn unmatched(
    turn: &mut Turn<'_, '_>,
    machine: &Machine,
    mut inst: Instance,
    from: &str,
) -> Result<Reply, Error> {
    if let Some(fb) = machine.fallback()
        && fb.name != from
    {
        turn.exit(machine, &mut inst, from, Some(&fb.name));
        inst.interrupted = Some(from.to_string());
        turn.enter(machine, &mut inst, &fb.name, Some(from));
        turn.settle(machine, &mut inst);
        return commit(
            turn,
            machine,
            inst,
            Some(from),
            format!("unmatched from {from}"),
        );
    }
    turn.exit(machine, &mut inst, from, None);
    inst.status = Status::Suspended;
    let key = inst.key();
    // One suspended slot: an older one is parked.
    if let Some(old) = turn.session.suspended.replace(key.clone())
        && old != key
        && let Some(mut o) = turn.host.store.instance(&old.machine, &old.id)?
        && o.status == Status::Suspended
        && o.holder.as_deref() == Some(&turn.session.key)
    {
        o.status = Status::Parked;
        o.holder = None;
        o.version += 1;
        o.updated = turn.now;
        // A conflict means another session changed it meanwhile: leave it.
        match turn.host.store.put_instance(&o) {
            Ok(()) => turn
                .notes
                .push(format!("Parked the previously suspended {}.", o.label())),
            Err(HostError::Conflict) => {}
            Err(e) => return Err(e.into()),
        }
    }
    turn.session.holding = None;
    turn.notes.push(format!(
        "Suspended {} {} at {from}. Handle the request, then fire resume from idle.",
        machine.instance.kind,
        inst.label()
    ));
    leave_to_idle(turn, machine, inst, from)
}

fn leave_to_idle(
    turn: &mut Turn<'_, '_>,
    machine: &Machine,
    mut inst: Instance,
    from: &str,
) -> Result<Reply, Error> {
    inst.version += 1;
    inst.updated = turn.now;
    if let Err(r) = save(turn, machine, &mut inst, Some(from), None)? {
        return Ok(r);
    }
    idle::reply(turn, true, None)
}

/// Save instance, history and session. `Err(reply)` when another session
/// wrote first (INST-8).
// @zen-impl: INST-8_AC-1
pub(crate) fn save(
    turn: &mut Turn<'_, '_>,
    machine: &Machine,
    inst: &mut Instance,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<Result<(), Reply>, Error> {
    match turn.host.store.put_instance(inst) {
        Ok(()) => {}
        Err(HostError::Conflict) => {
            let msg = format!(
                "{} {} was changed by another session; you are in idle",
                machine.instance.kind,
                inst.label()
            );
            return moved(turn, msg).map(Err);
        }
        Err(e) => return Err(e.into()),
    }
    let mut trace = Vec::new();
    trace.extend(turn.notes.iter().cloned());
    for p in &turn.passed {
        trace.push(format!("Passed through: {p}"));
    }
    trace.extend(turn.trace.iter().cloned());
    trace.extend(turn.failures.iter().cloned());
    let entry = HistoryEntry {
        at: turn.now,
        session: turn.session.key.clone(),
        event: turn.event.clone(),
        from: from.map(ToString::to_string),
        to: to.map(ToString::to_string),
        params: turn
            .params
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        trace,
    };
    turn.host
        .store
        .append_history(&inst.machine, &inst.id, &entry)?;
    turn.host.store.put_session(&turn.session)?;
    Ok(Ok(()))
}

/// Finish a transition: final state → completed + idle (IDLE-5); else the
/// entry block of where the instance rests.
// @zen-impl: IDLE-5_AC-1
pub(crate) fn commit(
    turn: &mut Turn<'_, '_>,
    machine: &Machine,
    mut inst: Instance,
    from: Option<&str>,
    mut arrived: String,
) -> Result<Reply, Error> {
    if !turn.passed.is_empty() {
        arrived.push_str(&format!(" via {}", turn.passed.join(", ")));
    }
    let is_final = machine.state(&inst.state).is_some_and(|s| s.is_final);
    if is_final {
        inst.status = Status::Completed;
        inst.holder = None;
        turn.session.holding = None;
    }
    turn.session.yielded = false;
    inst.version += 1;
    inst.updated = turn.now;
    let to = inst.state.clone();
    if let Err(r) = save(turn, machine, &mut inst, from, Some(&to))? {
        return Ok(r);
    }
    if is_final {
        return idle::after_final(turn, machine, &inst, arrived);
    }
    Ok(block(
        turn,
        machine,
        &inst,
        Some(&arrived),
        true,
        false,
        None,
    ))
}

/// `Took over from session K (last active …)` for the header (INST-6).
// @zen-impl: INST-6_AC-1
pub(crate) fn takeover_note(turn: &mut Turn<'_, '_>, holder: &str) -> Result<String, Error> {
    let last = turn.host.store.session(holder)?.map(|s| s.last_active);
    Ok(match last {
        Some(t) => format!(
            "Took over from session {holder} (last active {})",
            format_utc(t)
        ),
        None => format!("Took over from session {holder}"),
    })
}
