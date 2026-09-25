//! Lower `{type, params}` actions and guards (CFG-4, CFG-5, DEC-4..7).
// @zen-component: CFG-Lower

use std::path::Path;

use smllm_core::SmallMap;
use smllm_core::model::{ActionDef, GuardDef, Prompt, Value};

use crate::lower::checker::Checker;
use crate::source::{ActionSrc, Actions, CommandParams, GuardSrc, Run, StringOr};

/// Fences the agent text must not contain (TURN-12).
const FENCES: [&str; 3] = ["</smllm>", "</instructions>", "</events>"];

/// Where prompt files live and whether to inline them.
pub(crate) struct Files<'a> {
    pub dir: &'a Path,
    /// `smllm compile`: read prompt files now (browser hosts have no files).
    pub inline: bool,
}

impl Files<'_> {
    pub(crate) fn resolve(&self, rel: &str) -> String {
        self.dir.join(rel).display().to_string()
    }
}

/// Warn when author text contains smllm's fences (TURN-12).
// @zen-impl: TURN-12_AC-2
pub(crate) fn check_fences(c: &mut Checker<'_>, path: &[String], text: &str) {
    for f in FENCES {
        if text.contains(f) {
            c.warning(
                path,
                format!("text contains `{f}`, which smllm uses to fence agent text"),
                Some("reword it so the agent can tell smllm's text from yours"),
                "TURN-12",
            );
        }
    }
}

/// Lower an action list. `set_ref_ok`: only transition actions of `on`.
// @zen-impl: CFG-4_AC-1
pub(crate) fn lower_actions(
    c: &mut Checker<'_>,
    files: &Files<'_>,
    path: &[String],
    src: Option<&Actions>,
    set_ref_ok: bool,
) -> Vec<ActionDef> {
    let Some(src) = src else { return Vec::new() };
    let many = src.0.len() > 1;
    let mut out = Vec::new();
    for (i, a) in src.0.iter().enumerate() {
        let mut p = path.to_vec();
        if many {
            p.push(format!("[{i}]"));
        }
        let lowered = match a {
            StringOr::Str(s) if s == "setRef" => Some(ActionDef::SetRef),
            StringOr::Str(s) => {
                c.error(
                    &p,
                    format!("unknown action `{s}`"),
                    Some("write actions as {type, params}; the only param-less action is setRef"),
                    "CFG-4",
                );
                None
            }
            StringOr::Obj(ActionSrc::SetRef) => Some(ActionDef::SetRef),
            StringOr::Obj(ActionSrc::Prompt(pp)) => {
                lower_prompt(c, files, &p, pp.text.as_deref(), pp.file.as_deref())
            }
            StringOr::Obj(ActionSrc::Command(cp)) => Some(ActionDef::Host {
                kind: "command".to_string(),
                params: lower_command(c, files, &p, cp),
            }),
        };
        if matches!(lowered, Some(ActionDef::SetRef)) && !set_ref_ok {
            c.error(
                &p,
                "setRef runs only in a transition's actions under `on`".to_string(),
                Some("it takes the ref from the event's ref param, which entry, exit and always have none of"),
                "CFG-4",
            );
            continue;
        }
        out.extend(lowered);
    }
    out
}

fn lower_prompt(
    c: &mut Checker<'_>,
    files: &Files<'_>,
    path: &[String],
    text: Option<&str>,
    file: Option<&str>,
) -> Option<ActionDef> {
    match (text, file) {
        (Some(t), None) => {
            check_fences(c, path, t);
            Some(ActionDef::Prompt(Prompt::Text(t.to_string())))
        }
        (None, Some(f)) => {
            let full = files.dir.join(f);
            match std::fs::read_to_string(&full) {
                Ok(t) => {
                    check_fences(c, path, &t);
                    Some(ActionDef::Prompt(if files.inline {
                        Prompt::Text(t)
                    } else {
                        Prompt::File(full.display().to_string())
                    }))
                }
                Err(e) => {
                    c.error(
                        path,
                        format!("prompt file {f}: {e}"),
                        Some("paths are relative to the machine file"),
                        "CFG-4",
                    );
                    None
                }
            }
        }
        _ => {
            c.error(
                path,
                "a prompt needs exactly one of text, file".to_string(),
                None,
                "CFG-4",
            );
            None
        }
    }
}

/// The implied `enter-<STATE>.md` prompt, when the file exists or at request
/// time (CFG-4 default).
pub(crate) fn default_prompt(
    c: &mut Checker<'_>,
    files: &Files<'_>,
    path: &[String],
    state: &str,
) -> Option<ActionDef> {
    let full = files.dir.join(format!("enter-{state}.md"));
    if files.inline {
        let t = std::fs::read_to_string(&full).ok()?;
        check_fences(c, path, &t);
        return Some(ActionDef::Prompt(Prompt::Text(t)));
    }
    if let Ok(t) = std::fs::read_to_string(&full) {
        check_fences(c, path, &t);
    }
    Some(ActionDef::Prompt(Prompt::DefaultFile(
        full.display().to_string(),
    )))
}

fn lower_command(
    c: &mut Checker<'_>,
    files: &Files<'_>,
    path: &[String],
    cp: &CommandParams,
) -> SmallMap<Value> {
    let mut params = SmallMap::new();
    match &cp.run {
        Run::Shell(s) => {
            if s.trim().is_empty() {
                c.error(path, "command run is empty".to_string(), None, "DEC-4");
            }
            params.insert("run", Value::Str(s.clone()));
        }
        Run::Exec(argv) => {
            if argv.is_empty() {
                c.error(
                    path,
                    "command run is an empty list".to_string(),
                    None,
                    "DEC-4",
                );
            }
            if argv.iter().any(|a| a.contains("$SMLLM_")) {
                c.warning(
                    path,
                    "a list-form run gets no shell, so $SMLLM_* is not expanded".to_string(),
                    Some("use the string form, or read the environment in a script"),
                    "DEC-9",
                );
            }
            params.insert("run", Value::List(argv.clone()));
        }
    }
    if let Some(t) = cp.timeout_secs {
        if t == 0 {
            c.error(
                path,
                "timeoutSecs must be at least 1".to_string(),
                None,
                "DEC-5",
            );
        }
        params.insert("timeoutSecs", Value::Int(t.min(i64::MAX as u64) as i64));
    }
    if let Some(d) = &cp.cwd {
        params.insert("cwd", Value::Str(files.resolve(d)));
    }
    params
}

/// Lower a guard (CFG-5); `states` for checking `visits`.
// @zen-impl: CFG-5_AC-1
pub(crate) fn lower_guard(
    c: &mut Checker<'_>,
    files: &Files<'_>,
    path: &[String],
    g: &GuardSrc,
    states: &[&str],
) -> Option<GuardDef> {
    match g {
        GuardSrc::Command(cp) => Some(GuardDef::Host {
            kind: "command".to_string(),
            params: lower_command(c, files, path, cp),
        }),
        GuardSrc::Visits(v) => {
            if !states.contains(&v.state.as_str()) {
                c.error(
                    path,
                    format!("visits counts unknown state {}", v.state),
                    None,
                    "CFG-5",
                );
            }
            if v.at_least == 0 {
                c.warning(
                    path,
                    "visits atLeast 0 is always true".to_string(),
                    None,
                    "CFG-5",
                );
            }
            Some(GuardDef::Visits {
                state: v.state.clone(),
                at_least: v.at_least,
            })
        }
    }
}
