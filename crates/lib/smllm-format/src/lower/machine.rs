//! Validate a parsed machine file and lower it to the core model (CFG).
// @zen-component: CFG-Lower

use std::collections::{HashMap, HashSet};

use smllm_core::BUILTINS;
use smllm_core::model::{
    ActionDef, Machine, On, Position, Prompt, SharedAction, State, Transition,
};

use crate::finding::Level;
use crate::lower::actions::{Files, check_fences, default_prompt, lower_actions, lower_guard};
use crate::lower::checker::Checker;
use crate::lower::events::{is_name, lower_events, lower_instance, require_ref};
use crate::source::{MachineFile, PositionSrc, StateType, StringOr, Transitions};
use crate::ypath;

/// Lower `file`; `None` when there are errors.
// @zen-impl: CFG-1_AC-1
pub(crate) fn lower(c: &mut Checker<'_>, files: &Files<'_>, file: &MachineFile) -> Option<Machine> {
    if file.meta.smllm != 1 {
        c.error(
            &ypath!["meta", "smllm"],
            format!("unsupported smllm format version {}", file.meta.smllm),
            Some("this smllm reads version 1"),
            "CFG-1",
        );
    }
    if !is_name(&file.id) {
        c.error(
            &ypath!["id"],
            format!("id {:?} must be letters, digits, _ and -", file.id),
            None,
            "CFG-1",
        );
    }
    if let Some(d) = &file.description {
        check_fences(c, &ypath!["description"], d);
    }
    let names: Vec<&str> = file.states.iter().map(|(n, _)| n).collect();
    if names.is_empty() {
        c.error(
            &ypath!["states"],
            "a state machine needs at least one state".to_string(),
            None,
            "CFG-1",
        );
    }
    if !names.contains(&file.initial.as_str()) {
        c.error(
            &ypath!["initial"],
            format!("initial state {} does not exist", file.initial),
            None,
            "CFG-3",
        );
    }
    let instance = lower_instance(c, file.meta.instance.as_ref());
    let mut events = lower_events(c, &file.meta);

    let mut states = Vec::new();
    let mut used_events = HashSet::new();
    let mut set_ref_events = Vec::new();
    let mut fallbacks = Vec::new();
    for (name, node) in file.states.iter() {
        let sp = ypath!["states", name];
        if name.is_empty() || name.chars().any(char::is_whitespace) {
            c.error(
                &sp,
                format!("state name {name:?} must not contain spaces"),
                None,
                "CFG-1",
            );
        }
        let meta = node.meta.clone().unwrap_or_default();
        let is_final = node.kind == Some(StateType::Final);
        if let Some(d) = &node.description {
            check_fences(c, &[sp.clone(), ypath!["description"]].concat(), d);
        }
        if is_final && (node.on.is_some() || node.always.is_some()) {
            c.error(
                &sp,
                "a final state has no on or always".to_string(),
                None,
                "CFG-12",
            );
        }
        if meta.fallback {
            fallbacks.push(name);
            if is_final {
                c.error(
                    &sp,
                    "the fallback state must not be final".to_string(),
                    None,
                    "CFG-13",
                );
            }
        }
        let mut entry = lower_actions(
            c,
            files,
            &[sp.clone(), ypath!["entry"]].concat(),
            node.entry.as_ref(),
            false,
        );
        if !entry.iter().any(|a| matches!(a, ActionDef::Prompt(_))) {
            entry.extend(default_prompt(c, files, &sp, name));
        }
        let exit = lower_actions(
            c,
            files,
            &[sp.clone(), ypath!["exit"]].concat(),
            node.exit.as_ref(),
            false,
        );

        let mut on = Vec::new();
        if let Some(src) = &node.on {
            for (event, ts) in src.iter() {
                let ep = [sp.clone(), ypath!["on", event]].concat();
                if BUILTINS.contains(&event) {
                    c.error(
                        &ep,
                        format!("{event} is a built-in event; its name is reserved"),
                        None,
                        "CFG-13",
                    );
                    continue;
                }
                used_events.insert(event.to_string());
                let transitions = lower_transitions(c, files, &ep, ts, &names, true);
                if transitions
                    .iter()
                    .any(|t| t.actions.contains(&ActionDef::SetRef))
                {
                    set_ref_events.push(event.to_string());
                }
                on.push(On {
                    event: event.to_string(),
                    transitions,
                });
            }
        }
        let always = match &node.always {
            Some(ts) => lower_transitions(
                c,
                files,
                &[sp.clone(), ypath!["always"]].concat(),
                ts,
                &names,
                false,
            ),
            None => Vec::new(),
        };
        if !always.is_empty() && !on.is_empty() {
            c.warning(
                &sp,
                "a state with always is left at once; its on events are never offered".to_string(),
                None,
                "DEC-2",
            );
        }

        // Per-state prompts reword declared params, never define them (CFG-8).
        // @zen-impl: CFG-8_AC-1
        let mut param_descriptions = smllm_core::SmallMap::new();
        if let Some(pd) = &meta.param_descriptions {
            for (event, prompts) in pd.iter() {
                let pp = [sp.clone(), ypath!["meta", "paramDescriptions", event]].concat();
                if !on.iter().any(|o| o.event == event) {
                    c.warning(
                        &pp,
                        format!("{name} has no transition on {event}"),
                        None,
                        "CFG-8",
                    );
                }
                let mut m = smllm_core::SmallMap::new();
                for (param, text) in prompts.iter() {
                    let declared = events.get(event).is_some_and(|d| d.param(param).is_some())
                        || param == instance.ref_param;
                    if !declared {
                        c.error(
                            &[pp.clone(), ypath![param]].concat(),
                            format!("{event} declares no param {param}"),
                            Some("declare params in meta.events; paramDescriptions only rewords their prompts"),
                            "CFG-8",
                        );
                    }
                    m.insert(param, text.clone());
                }
                param_descriptions.insert(event, m);
            }
        }

        states.push(State {
            name: name.to_string(),
            description: node.description.clone(),
            is_final,
            entry_point: meta.entry_point,
            fallback: meta.fallback,
            entry,
            exit,
            on,
            always,
            param_descriptions,
        });
    }
    if fallbacks.len() > 1 {
        c.error(
            &ypath!["states", fallbacks[1]],
            format!("only one state may be the fallback (also {})", fallbacks[0]),
            None,
            "CFG-13",
        );
    }

    let mut shared = Vec::new();
    for (i, sa) in file.meta.shared_actions.iter().flatten().enumerate() {
        let p = ypath!["meta", "sharedActions", format!("[{i}]")];
        for s in &sa.states {
            if !names.contains(&s.as_str()) {
                c.error(
                    &p,
                    format!("sharedActions names unknown state {s}"),
                    None,
                    "CFG-11",
                );
            }
        }
        shared.push(SharedAction {
            states: sa.states.clone(),
            position: match sa.position {
                PositionSrc::Before => Position::Before,
                PositionSrc::After => Position::After,
            },
            entry: lower_actions(
                c,
                files,
                &[p.clone(), ypath!["entry"]].concat(),
                sa.entry.as_ref(),
                false,
            ),
            exit: lower_actions(
                c,
                files,
                &[p.clone(), ypath!["exit"]].concat(),
                sa.exit.as_ref(),
                false,
            ),
        });
    }

    for e in set_ref_events {
        require_ref(&mut events, &e, &instance);
    }
    for (name, _) in events.iter() {
        if !BUILTINS.contains(&name) && !used_events.contains(name) {
            c.warning(
                &ypath!["meta", "events", name],
                format!("event {name} is declared but no state uses it"),
                None,
                "CFG-9",
            );
        }
    }
    for (name, def) in events.iter() {
        if def.param(&instance.ref_param).is_some() && !def.params.is_empty() {
            let has_set_ref = states
                .iter()
                .flat_map(|s| s.on.iter())
                .filter(|o| o.event == name)
                .any(|o| {
                    o.transitions
                        .iter()
                        .any(|t| t.actions.contains(&ActionDef::SetRef))
                });
            if !has_set_ref {
                c.warning(
                    &ypath!["meta", "events", name],
                    format!(
                        "{name} has a param named like the ref param {} but never sets the ref",
                        instance.ref_param
                    ),
                    Some("rename the param, or add a setRef action"),
                    "CFG-13",
                );
            }
        }
    }

    let machine = Machine {
        id: file.id.clone(),
        description: file.description.clone(),
        initial: file.initial.clone(),
        instance,
        events,
        shared,
        states,
    };
    crate::lower::graph::check(c, &machine);
    if c.findings.count(Level::Error) > 0 {
        None
    } else {
        Some(machine)
    }
}

/// Lower `on.<event>` or `always`: targets exist; the last candidate has no
/// guard (CFG-13).
// @zen-impl: CFG-13_AC-1
fn lower_transitions(
    c: &mut Checker<'_>,
    files: &Files<'_>,
    path: &[String],
    src: &Transitions,
    states: &[&str],
    set_ref_ok: bool,
) -> Vec<Transition> {
    let many = src.0.len() > 1;
    let mut out = Vec::new();
    for (i, t) in src.0.iter().enumerate() {
        let mut p = path.to_vec();
        if many {
            p.push(format!("[{i}]"));
        }
        let lowered = match t {
            StringOr::Str(target) => Transition {
                target: Some(target.clone()),
                ..Transition::default()
            },
            StringOr::Obj(o) => Transition {
                target: o.target.clone(),
                guard: o.guard.as_ref().and_then(|g| {
                    lower_guard(c, files, &[p.clone(), ypath!["guard"]].concat(), g, states)
                }),
                actions: lower_actions(
                    c,
                    files,
                    &[p.clone(), ypath!["actions"]].concat(),
                    o.actions.as_ref(),
                    set_ref_ok,
                ),
                reenter: o.reenter.unwrap_or(false),
                description: o.description.clone(),
            },
        };
        if let Some(tg) = &lowered.target
            && !states.contains(&tg.as_str())
        {
            c.error(
                &p,
                format!("target state {tg} does not exist"),
                None,
                "CFG-3",
            );
        }
        if let Some(d) = &lowered.description {
            check_fences(c, &p, d);
        }
        for a in &lowered.actions {
            if let ActionDef::Prompt(Prompt::Text(t)) = a {
                check_fences(c, &p, t);
            }
        }
        out.push(lowered);
    }
    if out.last().is_some_and(|t| t.guard.is_some()) {
        let mut p = path.to_vec();
        if many {
            p.push(format!("[{}]", out.len() - 1));
        }
        c.error(
            &p,
            "the last transition has a guard, so nothing happens when every guard fails"
                .to_string(),
            Some("end the list with an unguarded transition (e.g. back to this state)"),
            "CFG-13",
        );
    }
    out
}

/// Reachable states from the initial, entry points and fallback.
pub(crate) fn reachable(m: &Machine) -> HashSet<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut todo: Vec<String> = vec![m.initial.clone()];
    todo.extend(
        m.states
            .iter()
            .filter(|s| s.entry_point || s.fallback)
            .map(|s| s.name.clone()),
    );
    let edges: HashMap<&str, Vec<&str>> = m
        .states
        .iter()
        .map(|s| {
            let ts =
                s.on.iter()
                    .flat_map(|o| o.transitions.iter())
                    .chain(s.always.iter());
            (
                s.name.as_str(),
                ts.filter_map(|t| t.target.as_deref()).collect(),
            )
        })
        .collect();
    while let Some(s) = todo.pop() {
        if !seen.insert(s.clone()) {
            continue;
        }
        for t in edges.get(s.as_str()).into_iter().flatten() {
            todo.push((*t).to_string());
        }
    }
    seen
}
