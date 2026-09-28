//! Read machine files and `config.toml`s; combine user + project configs
//! (CLI config lookup, D26).
// @zen-component: CFG-Load

use std::path::{Path, PathBuf};

use serde::Deserialize;
use smllm_core::model::{ActionDef, Config, Machine};

use crate::finding::{Finding, Findings, Severity};
use crate::lower::{Checker, Files, lower, lower_prompt};
use crate::source::MachineFile;

/// Which config a file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// `~/.config/smllm/config.toml`.
    User,
    /// The nearest `.smllm/config.toml`.
    Project,
    /// `--config` / `SMLLM_CONFIG`: the only one.
    Explicit,
}

/// A config file to load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigFile {
    /// Path to `config.toml`.
    pub path: PathBuf,
    /// Where it came from.
    pub origin: Origin,
}

/// How much a load does besides lowering (PLAN-004 D4-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// What the engine needs, for every command but `validate` and `compile`:
    /// prompt files are checked to exist, not read. Warnings that need their
    /// text are skipped.
    Run,
    /// `smllm validate`: every check, prompt files read.
    Check,
    /// `smllm compile`: every check, and prompt files inlined (browser hosts
    /// have no files).
    Inline,
}

/// Where a loaded machine lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineSource {
    /// Machine id.
    pub id: String,
    /// Its YAML file.
    pub file: PathBuf,
    /// The config that lists it.
    pub config: PathBuf,
    /// Its instances: that config's `[state] dir`, default `state/` beside
    /// it (STO-1).
    pub state_dir: PathBuf,
}

/// The combined, lowered config.
#[derive(Debug, Clone, Default)]
pub struct Loaded {
    /// For the engine; machines with errors are left out.
    pub config: Config,
    /// Where each machine came from.
    pub machines: Vec<MachineSource>,
    /// Everything found.
    pub findings: Findings,
    /// Every file the load read or checked, found or not: the configs, their
    /// machine files and the prompt files those name. The load is stale when
    /// any of them changes (the MCP server's cache, HOST-16).
    pub inputs: Vec<PathBuf>,
}

impl Loaded {
    /// The source of machine `id`.
    pub fn source(&self, id: &str) -> Option<&MachineSource> {
        self.machines.iter().find(|m| m.id == id)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct ConfigToml {
    #[serde(default)]
    machines: MachinesToml,
    #[serde(default)]
    idle: Option<IdleToml>,
    #[serde(default)]
    state: StateToml,
    /// `[harness.<name>] without = [...]`: parts the user declined (read by
    /// `smllm harness`).
    #[serde(default)]
    #[allow(dead_code)]
    harness: Option<toml::Table>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct MachinesToml {
    #[serde(default)]
    files: Vec<String>,
}

/// `[state] dir`: where this file's machines keep their instances,
/// relative to the file (or absolute); default `state` (STO-1).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct StateToml {
    #[serde(default)]
    dir: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct IdleToml {
    #[serde(default)]
    on_enter: Option<PromptToml>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct PromptToml {
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

fn finding(
    level: Severity,
    file: &Path,
    line: Option<usize>,
    message: String,
    rule: &'static str,
) -> Finding {
    Finding {
        level,
        file: file.to_path_buf(),
        line,
        path: None,
        message,
        hint: None,
        rule,
    }
}

/// Parse and lower one machine file.
// @zen-impl: CFG-14_AC-1
pub fn load_machine(path: &Path, mode: Mode) -> (Option<Machine>, Findings) {
    let (machine, findings, _) = load_machine_inputs(path, mode);
    (machine, findings)
}

/// [`load_machine`], and the prompt files it read or checked.
fn load_machine_inputs(path: &Path, mode: Mode) -> (Option<Machine>, Findings, Vec<PathBuf>) {
    let mut findings = Findings::default();
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            findings.push(finding(
                Severity::Error,
                path,
                None,
                format!("cannot read: {e}"),
                "CFG-14",
            ));
            return (None, findings, Vec::new());
        }
    };
    // YAML syntax first (one finding: the document cannot be read further).
    let mut doc: serde_json::Value = match serde_saphyr::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            let line = e.location().map(|l| l.line() as usize).filter(|l| *l > 0);
            findings.push(finding(
                Severity::Error,
                path,
                line,
                parse_message(&e.to_string()),
                "CFG-1",
            ));
            return (None, findings, Vec::new());
        }
    };
    // Then the whole shape, every problem at once (CFG-14).
    let mut shape = Checker::new(path, &text);
    crate::shape::check(&mut shape, &doc);
    if shape.findings.has_errors() {
        findings.extend(shape.findings);
        return (None, findings, Vec::new());
    }
    crate::shape::normalise(&mut doc);
    let parsed: MachineFile = match serde_json::from_value(doc) {
        Ok(m) => m,
        Err(e) => {
            // The shape check should have caught it; report what serde says.
            findings.push(finding(Severity::Error, path, None, e.to_string(), "CFG-1"));
            return (None, findings, Vec::new());
        }
    };
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut c = Checker::new(path, &text);
    let files = Files::new(dir, mode);
    let machine = lower(&mut c, &files, &parsed);
    findings.extend(c.findings);
    (machine, findings, files.probed())
}

/// First line of serde-saphyr's message without its `error: line N column M:`
/// prefix. (Unsupported XState keys never reach serde: the shape check
/// reports them first, with their hint.)
fn parse_message(full: &str) -> String {
    let first = full.lines().next().unwrap_or(full);
    let msg = first.strip_prefix("error: ").unwrap_or(first);
    let msg = match msg.find(": ") {
        Some(i) if msg.starts_with("line ") => &msg[i + 2..],
        _ => msg,
    };
    // serde-saphyr suggests a library option; the author needs the key.
    if let Some(rest) = msg.strip_prefix("duplicate mapping key: ") {
        let key = rest.split(", set ").next().unwrap_or(rest);
        return format!("duplicate key `{key}`");
    }
    msg.to_string()
}

/// Load and combine config files, in order: user then project; the project
/// wins on a machine id clash, and its `[idle]` replaces the user's (D26).
// @zen-impl: CFG-15_AC-2
pub fn load_configs(files: &[ConfigFile], mode: Mode) -> Loaded {
    let mut out = Loaded::default();
    let mut idle: Option<Vec<ActionDef>> = None;
    for cf in files {
        out.inputs.push(cf.path.clone());
        let text = match std::fs::read_to_string(&cf.path) {
            Ok(t) => t,
            Err(e) => {
                out.findings.push(finding(
                    Severity::Error,
                    &cf.path,
                    None,
                    format!("cannot read: {e}"),
                    "CLI-3",
                ));
                continue;
            }
        };
        let parsed: ConfigToml = match toml::from_str(&text) {
            Ok(c) => c,
            Err(e) => {
                let line = e.span().map(|s| text[..s.start].matches('\n').count() + 1);
                out.findings.push(finding(
                    Severity::Error,
                    &cf.path,
                    line,
                    e.message().to_string(),
                    "CLI-3",
                ));
                // It may replace any of the machines so far: use none of them.
                let ids: Vec<String> = out.machines.drain(..).map(|m| m.id).collect();
                out.config.machines.clear();
                if !ids.is_empty() {
                    out.findings.push(finding(
                        Severity::Warning,
                        &cf.path,
                        None,
                        format!(
                            "machines {} from earlier configs are not loaded until this file is fixed (it may replace them)",
                            ids.join(", ")
                        ),
                        "CFG-15",
                    ));
                }
                continue;
            }
        };
        let dir = cf.path.parent().unwrap_or(Path::new("."));
        // A later `[idle]` table replaces an earlier one, even without on-enter.
        if let Some(i) = parsed.idle {
            idle = Some(match i.on_enter {
                Some(p) => {
                    let mut c = Checker::new(&cf.path, &text);
                    let files = Files::new(dir, mode);
                    let at = ["idle".to_string(), "on-enter".to_string()];
                    let prompt =
                        lower_prompt(&mut c, &files, &at, p.text.as_deref(), p.file.as_deref());
                    out.findings.extend(c.findings);
                    out.inputs.extend(files.probed());
                    prompt.into_iter().collect()
                }
                None => Vec::new(),
            });
        }
        let mut seen_here: Vec<String> = Vec::new();
        for rel in &parsed.machines.files {
            let file = dir.join(rel);
            let (machine, findings, prompts) = load_machine_inputs(&file, mode);
            out.findings.extend(findings);
            out.inputs.push(file.clone());
            out.inputs.extend(prompts);
            let Some(machine) = machine else {
                withdraw_replaced(&mut out, &file, &cf.path);
                continue;
            };
            if seen_here.contains(&machine.id) {
                out.findings.push(finding(
                    Severity::Error,
                    &cf.path,
                    None,
                    format!("two machines have id {}", machine.id),
                    "CFG-1",
                ));
                continue;
            }
            // Instances live in `<state dir>/<id>/`: on a
            // case-insensitive file system (macOS, Windows) `Dev` and `dev`
            // would share it.
            if let Some(other) = seen_here
                .iter()
                .find(|o| o.eq_ignore_ascii_case(&machine.id))
            {
                out.findings.push(finding(
                    Severity::Error,
                    &file,
                    None,
                    format!(
                        "machine id {} differs from {other} only in case; they would share a state directory on macOS and Windows",
                        machine.id
                    ),
                    "CFG-1",
                ));
                continue;
            }
            seen_here.push(machine.id.clone());
            let source = MachineSource {
                id: machine.id.clone(),
                file: file.clone(),
                config: cf.path.clone(),
                state_dir: dir.join(parsed.state.dir.as_deref().unwrap_or("state")),
            };
            if let Some(i) = out.config.machines.iter().position(|m| m.id == machine.id) {
                out.findings.push(finding(
                    Severity::Info,
                    &file,
                    None,
                    format!(
                        "machine {} here replaces the one in {}",
                        machine.id,
                        out.machines[i].file.display()
                    ),
                    "CFG-15",
                ));
                // Replaced in place: config order is kept.
                out.machines[i] = source;
                out.config.machines[i] = machine;
                continue;
            }
            out.machines.push(source);
            out.config.machines.push(machine);
        }
    }
    out.config.idle = idle.unwrap_or_default();
    out
}

/// A machine file that failed to load may be meant to replace a machine an
/// earlier config gave: that one is not used in its place, so its id stays
/// unconfigured (and sessions holding it wait, INST-7) until the file is
/// fixed. The id is read from the file's top-level `id:` line.
fn withdraw_replaced(out: &mut Loaded, file: &Path, config: &Path) {
    let Some(id) = declared_id(file) else { return };
    let Some(i) = out
        .machines
        .iter()
        .position(|m| m.id == id && m.config != config)
    else {
        return;
    };
    let earlier = out.machines.remove(i);
    out.config.machines.remove(i);
    out.findings.push(finding(
        Severity::Warning,
        file,
        None,
        format!(
            "machine {id} in {} is not used while this file, which replaces it, has errors",
            earlier.file.display()
        ),
        "CFG-15",
    ));
}

/// The value of a machine file's top-level `id:` line, if any.
fn declared_id(file: &Path) -> Option<String> {
    let text = std::fs::read_to_string(file).ok()?;
    let value = text.lines().find_map(|l| l.strip_prefix("id:"))?;
    let value = value.split(" #").next().unwrap_or(value).trim();
    let value = value.trim_matches(['"', '\'']);
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_messages_lose_the_location_prefix() {
        assert_eq!(
            parse_message("error: line 6 column 5: did not find expected key\n --> x"),
            "did not find expected key"
        );
        assert_eq!(parse_message("plain"), "plain");
        let m = parse_message(
            "error: line 3 column 1: duplicate mapping key: initial, set DuplicateKeyPolicy in Options if acceptable",
        );
        assert_eq!(m, "duplicate key `initial`");
    }
}
