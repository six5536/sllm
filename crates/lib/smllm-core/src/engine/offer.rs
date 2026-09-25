//! Offered events and their params: what the menu shows and what a call is
//! checked against (ENG-1, TURN-2, IDLE).
// @zen-component: ENG-Offers

use crate::host::Matcher;
use crate::model::{Config, EventDef, Machine, ParamSpec, State};
use crate::prelude::*;
use crate::record::Instance;

/// Built-in event names (IDLE-1).
pub const BUILTINS: [&str; 5] = ["enter", "resume", "park", "unmatched", "yield"];

/// An offered event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    /// Event name.
    pub name: String,
    /// Guidance.
    pub description: Option<String>,
    /// Params.
    pub params: Vec<ParamView>,
}

/// A param as shown and checked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParamView {
    /// Name.
    pub name: String,
    /// Required.
    pub required: bool,
    /// Allowed values.
    pub enum_values: Vec<String>,
    /// Pattern.
    pub pattern: Option<String>,
    /// Prompt.
    pub description: Option<String>,
}

impl ParamView {
    fn from_spec(spec: &ParamSpec, prompt: Option<&String>) -> Self {
        Self {
            name: spec.name.clone(),
            required: spec.required,
            enum_values: spec.enum_values.clone(),
            pattern: spec.pattern.clone(),
            description: prompt.cloned().or_else(|| spec.description.clone()),
        }
    }
}

/// Guidance for a built-in: the machine's override, else `default` (IDLE-1).
// @zen-impl: IDLE-1_AC-1
fn builtin_description(machine: Option<&Machine>, name: &str, default: String) -> Option<String> {
    machine
        .and_then(|m| m.events.get(name))
        .and_then(|e| e.description.clone())
        .or(Some(default))
}

/// Events offered in a machine state: its `on` events, then the built-ins
/// (ENG-1). `state` is `None` when the saved state no longer exists (IDLE-3).
// @zen-impl: ENG-1_AC-1
pub(crate) fn machine_offers(
    machine: &Machine,
    state: Option<&State>,
    inst: &Instance,
) -> Vec<Offer> {
    let mut offers = Vec::new();
    let noun = &machine.instance.noun;
    let label = inst.label();
    if let Some(state) = state {
        if state.is_final {
            return offers;
        }
        let empty = EventDef::default();
        for on in &state.on {
            let def = machine.events.get(&on.event).unwrap_or(&empty);
            let prompts = state.param_descriptions.get(&on.event);
            offers.push(Offer {
                name: on.event.clone(),
                description: on
                    .description()
                    .map(ToString::to_string)
                    .or_else(|| def.description.clone()),
                params: def
                    .params
                    .iter()
                    .map(|p| ParamView::from_spec(p, prompts.and_then(|m| m.get(&p.name))))
                    .collect(),
            });
        }
        if state.fallback
            && let Some(back) = &inst.interrupted
        {
            offers.push(Offer {
                name: "resume".to_string(),
                description: builtin_description(
                    Some(machine),
                    "resume",
                    format!("Return to {back}."),
                ),
                params: Vec::new(),
            });
        }
        offers.push(Offer {
            name: "yield".to_string(),
            description: builtin_description(
                Some(machine),
                "yield",
                format!("Stop for now and stay in {}.", state.name),
            ),
            params: vec![ParamView {
                name: "note".to_string(),
                description: Some("What you are waiting for.".to_string()),
                ..ParamView::default()
            }],
        });
    }
    offers.push(Offer {
        name: "park".to_string(),
        description: builtin_description(
            Some(machine),
            "park",
            format!("Put {noun} {label} aside and return to idle."),
        ),
        params: Vec::new(),
    });
    let detour = match machine.fallback() {
        Some(f) if state.is_none_or(|s| s.name != f.name) => {
            format!(
                "The request fits none of these; handle it in {}, then resume.",
                f.name
            )
        }
        _ => "The request fits none of these; handle it from idle, then resume.".to_string(),
    };
    offers.push(Offer {
        name: "unmatched".to_string(),
        description: builtin_description(Some(machine), "unmatched", detour),
        params: Vec::new(),
    });
    offers
}

/// Events offered in idle: `enter`, and `resume` when an instance is suspended.
pub(crate) fn idle_offers(config: &Config, suspended: Option<(&Machine, &Instance)>) -> Vec<Offer> {
    let mut ref_params: Vec<&str> = Vec::new();
    for m in &config.machines {
        if !ref_params.contains(&m.instance.ref_param.as_str()) {
            ref_params.push(&m.instance.ref_param);
        }
    }
    let mut offers = vec![Offer {
        name: "enter".to_string(),
        description: Some("Start a new instance of a state machine, or continue one.".to_string()),
        params: vec![
            ParamView {
                name: "stateMachine".to_string(),
                required: true,
                enum_values: config.machines.iter().map(|m| m.id.clone()).collect(),
                description: Some("The state machine to enter.".to_string()),
                ..ParamView::default()
            },
            ParamView {
                name: ref_params.join(" | "),
                description: Some(
                    "The id param its state machine names above: an existing id or ref \
                     continues that instance, a new ref starts one with it; omit to start a \
                     new one."
                        .to_string(),
                ),
                ..ParamView::default()
            },
            ParamView {
                name: "state".to_string(),
                description: Some(
                    "Enter here instead: an entry point. Omit for the initial state (new) or \
                     the saved state (existing)."
                        .to_string(),
                ),
                ..ParamView::default()
            },
        ],
    }];
    if let Some((m, inst)) = suspended {
        offers.push(Offer {
            name: "resume".to_string(),
            description: builtin_description(
                Some(m),
                "resume",
                format!(
                    "Return to {} {} ({}) at {}.",
                    m.instance.noun,
                    inst.label(),
                    m.id,
                    inst.state
                ),
            ),
            params: Vec::new(),
        });
    }
    offers
}

/// Check `given` against an offer's params (CFG-16, TURN-3). Returns the
/// params in declaration order, or the error message.
// @zen-impl: TURN-3_AC-1
pub(crate) fn check_params(
    offer: &Offer,
    given: &[(String, String)],
    matcher: &dyn Matcher,
) -> Result<Vec<(String, String)>, String> {
    for (name, _) in given {
        if !offer.params.iter().any(|p| &p.name == name) {
            return Err(if offer.params.is_empty() {
                format!("{} takes no params, but got {name}", offer.name)
            } else {
                let names: Vec<&str> = offer.params.iter().map(|p| p.name.as_str()).collect();
                format!(
                    "{} has no param {name} (params: {})",
                    offer.name,
                    names.join(", ")
                )
            });
        }
    }
    let mut out = Vec::new();
    for p in &offer.params {
        let Some((_, value)) = given.iter().find(|(n, _)| n == &p.name) else {
            if p.required {
                return Err(format!("{} needs param {}", offer.name, p.name));
            }
            continue;
        };
        check_value(&offer.name, p, value, matcher)?;
        out.push((p.name.clone(), value.clone()));
    }
    Ok(out)
}

/// Check one value against `enum` and `pattern`.
pub(crate) fn check_value(
    event: &str,
    p: &ParamView,
    value: &str,
    matcher: &dyn Matcher,
) -> Result<(), String> {
    if !p.enum_values.is_empty() && !p.enum_values.iter().any(|v| v == value) {
        return Err(format!(
            "{event} param {} must be one of: {} (got \"{value}\")",
            p.name,
            p.enum_values.join(", ")
        ));
    }
    if let Some(pat) = &p.pattern {
        match matcher.is_match(pat, value) {
            Ok(true) => {}
            Ok(false) => {
                return Err(format!(
                    "{event} param {} must match {pat} (got \"{value}\")",
                    p.name
                ));
            }
            Err(e) => return Err(format!("{event} param {}: bad pattern {pat}: {e}", p.name)),
        }
    }
    Ok(())
}
