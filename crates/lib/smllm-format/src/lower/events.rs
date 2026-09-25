//! `meta.instance` and `meta.events` (CFG-6, CFG-7, CFG-16, IDLE-1).
// @zen-component: CFG-Lower

use smllm_core::model::{EventDef, InstanceSpec, ParamSpec};
use smllm_core::{BUILTINS, SmallMap};

use crate::lower::checker::Checker;
use crate::source::{InstanceMeta, MachineMeta, ParamsSchema};
use crate::ypath;

/// Names the `enter` event uses itself (CFG-13).
const ENTER_PARAMS: [&str; 2] = ["stateMachine", "state"];

pub(crate) fn valid_pattern(c: &mut Checker<'_>, path: &[String], pattern: &str) -> bool {
    match regex::Regex::new(pattern) {
        Ok(_) => true,
        Err(e) => {
            let first = e.to_string().lines().last().unwrap_or_default().to_string();
            c.error(
                path,
                format!("bad pattern {pattern}: {first}"),
                None,
                "CFG-7",
            );
            false
        }
    }
}

/// `meta.instance` → [`InstanceSpec`] (CFG-6).
// @zen-impl: CFG-6_AC-1
pub(crate) fn lower_instance(c: &mut Checker<'_>, src: Option<&InstanceMeta>) -> InstanceSpec {
    let mut spec = InstanceSpec::default();
    let Some(src) = src else { return spec };
    if let Some(n) = &src.noun {
        spec.noun = n.clone();
    }
    if let Some(r) = &src.r#ref {
        if let Some(p) = &r.param {
            spec.ref_param = p.clone();
        }
        spec.ref_description = r.description.clone();
        if let Some(pat) = &r.pattern
            && valid_pattern(c, &ypath!["meta", "instance", "ref", "pattern"], pat)
        {
            spec.ref_pattern = Some(pat.clone());
        }
    }
    if ENTER_PARAMS.contains(&spec.ref_param.as_str()) {
        c.error(
            &ypath!["meta", "instance", "ref", "param"],
            format!(
                "ref param {} clashes with enter's own param",
                spec.ref_param
            ),
            Some("pick another name, e.g. issueId"),
            "CFG-13",
        );
    }
    if !is_name(&spec.ref_param) {
        c.error(
            &ypath!["meta", "instance", "ref", "param"],
            format!(
                "ref param {:?} must be letters, digits and _",
                spec.ref_param
            ),
            None,
            "CFG-6",
        );
    }
    spec
}

pub(crate) fn is_name(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// `meta.events` → event defs (CFG-7); built-ins may set only guidance (IDLE-1).
// @zen-impl: CFG-7_AC-1
// @zen-impl: IDLE-1_AC-1
pub(crate) fn lower_events(c: &mut Checker<'_>, meta: &MachineMeta) -> SmallMap<EventDef> {
    let mut out = SmallMap::new();
    let Some(events) = &meta.events else {
        return out;
    };
    for (name, ev) in events.iter() {
        let path = ypath!["meta", "events", name];
        if BUILTINS.contains(&name) && ev.params.is_some() {
            c.error(
                &path,
                format!("built-in event {name} may override only its description, not its params"),
                None,
                "IDLE-1",
            );
        }
        let params = match &ev.params {
            Some(schema) if !BUILTINS.contains(&name) => lower_params(c, &path, schema),
            _ => Vec::new(),
        };
        out.insert(
            name,
            EventDef {
                description: ev.description.clone(),
                params,
            },
        );
    }
    out
}

fn lower_params(c: &mut Checker<'_>, path: &[String], schema: &ParamsSchema) -> Vec<ParamSpec> {
    let mut p = path.to_vec();
    p.push("params".into());
    if schema.kind != "object" {
        c.error(
            &p,
            format!("params type must be object, not {}", schema.kind),
            None,
            "CFG-7",
        );
    }
    let mut out = Vec::new();
    for (name, prop) in schema.properties.iter() {
        let mut pp = p.clone();
        pp.extend(["properties".to_string(), name.to_string()]);
        if prop.kind != "string" {
            c.error(
                &pp,
                format!("param {name} has type {}; v1 params are strings", prop.kind),
                Some("use type: string, with enum or pattern to constrain it"),
                "CFG-16",
            );
        }
        if !is_name(name) {
            c.error(
                &pp,
                format!("param name {name:?} must be letters, digits, _ and -"),
                None,
                "CFG-7",
            );
        }
        if ENTER_PARAMS.contains(&name) {
            c.warning(
                &pp,
                format!("param {name} shadows enter's own param"),
                None,
                "CFG-13",
            );
        }
        let enum_values = prop.enum_values.clone().unwrap_or_default();
        if prop.enum_values.as_ref().is_some_and(Vec::is_empty) {
            c.error(&pp, "enum is empty".to_string(), None, "CFG-7");
        }
        let pattern = prop.pattern.as_ref().filter(|pat| {
            let mut ppp = pp.clone();
            ppp.push("pattern".into());
            valid_pattern(c, &ppp, pat)
        });
        out.push(ParamSpec {
            name: name.to_string(),
            description: prop.description.clone(),
            required: schema.required.iter().any(|r| r == name),
            enum_values,
            pattern: pattern.cloned(),
        });
    }
    for r in &schema.required {
        if !schema.properties.contains_key(r) {
            let mut rp = p.clone();
            rp.push("required".into());
            c.error(
                &rp,
                format!("required param {r} is not in properties"),
                None,
                "CFG-7",
            );
        }
    }
    out
}

/// A `setRef` transition makes the ref param required on its event (CFG-9).
// @zen-impl: CFG-9_AC-1
pub(crate) fn require_ref(events: &mut SmallMap<EventDef>, event: &str, spec: &InstanceSpec) {
    let mut def = events.remove(event).unwrap_or_default();
    match def.params.iter_mut().find(|p| p.name == spec.ref_param) {
        Some(p) => {
            p.required = true;
            if p.pattern.is_none() {
                p.pattern = spec.ref_pattern.clone();
            }
            if p.description.is_none() {
                p.description = spec.ref_description.clone();
            }
        }
        None => def.params.insert(
            0,
            ParamSpec {
                name: spec.ref_param.clone(),
                description: spec.ref_description.clone(),
                required: true,
                enum_values: Vec::new(),
                pattern: spec.ref_pattern.clone(),
            },
        ),
    }
    events.insert(event, def);
}
