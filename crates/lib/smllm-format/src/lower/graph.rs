//! Whole-machine checks: reachability, final states, `always` loops
//! (CFG-12, CFG-13, DEC-2).
// @zen-component: CFG-Lower

use std::collections::BTreeMap;

use smllm_core::model::Machine;

use crate::finding::Level;
use crate::lower::checker::Checker;
use crate::lower::machine::reachable;
use crate::ypath;

// @zen-impl: CFG-12_AC-1
// @zen-impl: DEC-2_AC-2
pub(crate) fn check(c: &mut Checker<'_>, m: &Machine) {
    if m.states.is_empty() || m.state(&m.initial).is_none() {
        return;
    }
    let seen = reachable(m);
    for s in &m.states {
        if !seen.contains(&s.name) {
            c.warning(
                &ypath!["states", s.name],
                format!("state {} is unreachable", s.name),
                Some("add a transition to it, or mark it meta.entryPoint"),
                if s.is_final { "CFG-12" } else { "CFG-3" },
            );
        }
    }
    if !m.states.iter().any(|s| s.is_final) {
        c.add(
            Level::Info,
            &ypath!["states"],
            "no final state: instances of this machine never complete".to_string(),
            Some("fine for a machine that loops forever; add type: final to finish work"),
            "CFG-12",
        );
    }
    // A cycle made only of `always` edges would never rest (DEC-2).
    let always: BTreeMap<&str, Vec<&str>> = m
        .states
        .iter()
        .filter(|s| !s.always.is_empty())
        .map(|s| {
            (
                s.name.as_str(),
                // A targetless `always` stays put and fires again: a self-loop.
                s.always
                    .iter()
                    .map(|t| t.target.as_deref().unwrap_or(s.name.as_str()))
                    .collect(),
            )
        })
        .collect();
    let mut reported = Vec::new();
    for start in always.keys() {
        let mut stack = vec![(*start, vec![*start])];
        while let Some((node, path)) = stack.pop() {
            for next in always.get(node).into_iter().flatten() {
                if next == start {
                    let mut cycle = path.clone();
                    cycle.sort_unstable();
                    if !reported.contains(&cycle) {
                        c.error(
                            &ypath!["states", start, "always"],
                            format!("always transitions loop: {} → {start}", path.join(" → ")),
                            Some(
                                "an always state is never rested in; break the loop with an event",
                            ),
                            "DEC-2",
                        );
                        reported.push(cycle);
                    }
                } else if always.contains_key(next) && !path.contains(next) {
                    let mut p = path.clone();
                    p.push(next);
                    stack.push((next, p));
                }
            }
        }
    }
}
