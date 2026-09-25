//! `smllm graph`: states and transitions, guards shown (CLI-10).
// @zen-component: CLI-Graph

use std::path::Path;

use serde_json::{Value, json};
use smllm_core::model::{ActionDef, GuardDef, Machine, Transition};

use crate::cli::GraphArgs;
use crate::error::{Error, Result};
use crate::output::{self, EXIT_OK};
use crate::runtime::Runtime;

fn guard_label(g: &GuardDef) -> String {
    match g {
        GuardDef::Visits { state, at_least } => format!("visits {state} ≥ {at_least}"),
        GuardDef::Host { kind, params } => match params.get("run") {
            Some(smllm_core::model::Value::Str(s)) => format!("{kind} `{s}`"),
            Some(smllm_core::model::Value::List(l)) => format!("{kind} [{}]", l.join(" ")),
            _ => kind.clone(),
        },
    }
}

/// Every (event, transition) of a state, `always` as event `always`.
fn edges(s: &smllm_core::model::State) -> Vec<(&str, &Transition)> {
    let mut out: Vec<(&str, &Transition)> = Vec::new();
    for o in &s.on {
        out.extend(o.transitions.iter().map(|t| (o.event.as_str(), t)));
    }
    out.extend(s.always.iter().map(|t| ("always", t)));
    out
}

fn text(m: &Machine) -> String {
    let mut out = format!("{} (initial {})\n", m.id, m.initial);
    for s in &m.states {
        let mut tags = Vec::new();
        if s.entry_point {
            tags.push("entry point");
        }
        if s.fallback {
            tags.push("fallback");
        }
        if s.is_final {
            tags.push("final");
        }
        let tags = if tags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", tags.join(", "))
        };
        out.push_str(&format!("  {}{tags}\n", s.name));
        for (event, t) in edges(s) {
            let target = t.target.as_deref().unwrap_or("(stays)");
            let mut line = format!("    {event}");
            if let Some(g) = &t.guard {
                line.push_str(&format!(" [{}]", guard_label(g)));
            }
            line.push_str(&format!(" → {target}"));
            if t.reenter {
                line.push_str(" (reenter)");
            }
            if t.actions.contains(&ActionDef::SetRef) {
                line.push_str(" (setRef)");
            }
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

fn mermaid(m: &Machine) -> String {
    let mut out = format!(
        "---\ntitle: {}\n---\nstateDiagram-v2\n    [*] --> {}\n",
        m.id, m.initial
    );
    for s in &m.states {
        for (event, t) in edges(s) {
            let target = t.target.as_deref().unwrap_or(&s.name);
            let label = match &t.guard {
                Some(g) => format!("{event} [{}]", guard_label(g).replace(['"', '`', ':'], "")),
                None => event.to_string(),
            };
            out.push_str(&format!("    {} --> {target}: {label}\n", s.name));
        }
        if s.is_final {
            out.push_str(&format!("    {} --> [*]\n", s.name));
        }
    }
    out
}

fn to_json(m: &Machine) -> Value {
    let states: Vec<Value> = m
        .states
        .iter()
        .map(|s| {
            let ts: Vec<Value> = edges(s)
                .into_iter()
                .map(|(event, t)| {
                    json!({
                        "event": event,
                        "target": t.target,
                        "guard": t.guard.as_ref().map(guard_label),
                        "reenter": t.reenter,
                    })
                })
                .collect();
            json!({
                "name": s.name,
                "final": s.is_final,
                "entryPoint": s.entry_point,
                "fallback": s.fallback,
                "transitions": ts,
            })
        })
        .collect();
    json!({ "id": m.id, "initial": m.initial, "states": states })
}

/// `smllm graph [ID] [--json|--mermaid]`.
// @zen-impl: CLI-10_AC-1
pub fn graph(args: &GraphArgs, explicit: Option<&Path>) -> Result<u8> {
    let cwd = std::env::current_dir().map_err(|e| Error::io(Path::new("."), e))?;
    let rt = Runtime::lookup(explicit, &cwd)?;
    let machines: Vec<&Machine> = match &args.id {
        Some(id) => vec![
            rt.loaded
                .config
                .machine(id)
                .ok_or_else(|| Error::msg(format!("no state machine {id}")))?,
        ],
        None => rt.loaded.config.machines.iter().collect(),
    };
    if args.json {
        output::json(
            &json!({ "machines": machines.iter().map(|m| to_json(m)).collect::<Vec<_>>() }),
        )?;
    } else if args.mermaid {
        output::text(
            &machines
                .iter()
                .map(|m| mermaid(m))
                .collect::<Vec<_>>()
                .join("\n"),
        )?;
    } else {
        output::text(
            &machines
                .iter()
                .map(|m| text(m))
                .collect::<Vec<_>>()
                .join("\n"),
        )?;
    }
    Ok(EXIT_OK)
}
