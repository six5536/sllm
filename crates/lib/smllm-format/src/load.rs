//! Read machine files and `config.toml`s; combine user + project configs
//! (CLI config lookup, D26).
// @zen-component: CFG-Load

use std::path::{Path, PathBuf};

use serde::Deserialize;
use smllm_core::model::{ActionDef, Config, Machine, Prompt};

use crate::finding::{Finding, Findings, Level};
use crate::lower::{Checker, Files, lower};
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

/// Where a loaded machine lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineSource {
    /// Machine id.
    pub id: String,
    /// Its YAML file.
    pub file: PathBuf,
    /// The config that lists it.
    pub config: PathBuf,
    /// Its instances: `state/` beside that config (STO-1).
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
    level: Level,
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

/// Parse and lower one machine file. `inline` reads prompt files now
/// (`smllm compile`).
// @zen-impl: CFG-14_AC-1
pub fn load_machine(path: &Path, inline: bool) -> (Option<Machine>, Findings) {
    let mut findings = Findings::default();
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            findings.push(finding(
                Level::Error,
                path,
                None,
                format!("cannot read: {e}"),
                "CFG-14",
            ));
            return (None, findings);
        }
    };
    // YAML syntax first (one finding: the document cannot be read further).
    let mut doc: serde_json::Value = match serde_saphyr::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            let line = e.location().map(|l| l.line() as usize).filter(|l| *l > 0);
            let (message, hint) = parse_message(&e.to_string());
            findings.push(Finding {
                hint,
                ..finding(Level::Error, path, line, message, "CFG-1")
            });
            return (None, findings);
        }
    };
    // Then the whole shape, every problem at once (CFG-14).
    let mut shape = Checker::new(path, &text);
    crate::shape::check(&mut shape, &doc);
    if shape.findings.has_errors() {
        findings.extend(shape.findings);
        return (None, findings);
    }
    crate::shape::normalise(&mut doc);
    let parsed: MachineFile = match serde_json::from_value(doc) {
        Ok(m) => m,
        Err(e) => {
            // The shape check should have caught it; report what serde says.
            findings.push(finding(Level::Error, path, None, e.to_string(), "CFG-1"));
            return (None, findings);
        }
    };
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut c = Checker::new(path, &text);
    let machine = lower(&mut c, &Files { dir, inline }, &parsed);
    findings.extend(c.findings);
    (machine, findings)
}

/// First line of serde-saphyr's message without its `error: line N column M:`
/// prefix, plus a hint for XState features smllm v1 does not support (CFG-2).
// @zen-impl: CFG-2_AC-1
fn parse_message(full: &str) -> (String, Option<String>) {
    let first = full.lines().next().unwrap_or(full);
    let msg = first.strip_prefix("error: ").unwrap_or(first);
    let msg = match msg.find(": ") {
        Some(i) if msg.starts_with("line ") => &msg[i + 2..],
        _ => msg,
    };
    // serde-saphyr suggests a library option; the author needs the key.
    if let Some(rest) = msg.strip_prefix("duplicate mapping key: ") {
        let key = rest.split(", set ").next().unwrap_or(rest);
        return (format!("duplicate key `{key}`"), None);
    }
    let unsupported = [
        "`states`",
        "`parallel`",
        "`history`",
        "`after`",
        "`invoke`",
        "`context`",
        "`output`",
        "`tags`",
        "`assign`",
    ];
    let hint = unsupported
        .iter()
        .find(|u| msg.contains(&format!("field {u}")) || msg.contains(&format!("variant {u}")))
        .map(|u| format!("{} is XState, but not in smllm v1's subset (flat atomic/final states; no delays, invocations, context)", u.trim_matches('`')));
    (msg.to_string(), hint)
}

/// Load and combine config files, in order: user then project; the project
/// wins on a machine id clash, and its `[idle]` replaces the user's (D26).
// @zen-impl: CFG-15_AC-2
pub fn load_configs(files: &[ConfigFile], inline: bool) -> Loaded {
    let mut out = Loaded::default();
    let mut idle: Option<Vec<ActionDef>> = None;
    for cf in files {
        let text = match std::fs::read_to_string(&cf.path) {
            Ok(t) => t,
            Err(e) => {
                out.findings.push(finding(
                    Level::Error,
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
                    Level::Error,
                    &cf.path,
                    line,
                    e.message().to_string(),
                    "CLI-3",
                ));
                continue;
            }
        };
        let dir = cf.path.parent().unwrap_or(Path::new("."));
        // A later `[idle]` table replaces an earlier one, even without on-enter.
        if let Some(i) = parsed.idle {
            idle = Some(match i.on_enter {
                Some(p) => idle_prompt(&mut out.findings, &cf.path, dir, p, inline),
                None => Vec::new(),
            });
        }
        let mut seen_here: Vec<String> = Vec::new();
        for rel in &parsed.machines.files {
            let file = dir.join(rel);
            let (machine, findings) = load_machine(&file, inline);
            out.findings.extend(findings);
            let Some(machine) = machine else { continue };
            if seen_here.contains(&machine.id) {
                out.findings.push(finding(
                    Level::Error,
                    &cf.path,
                    None,
                    format!("two machines have id {}", machine.id),
                    "CFG-1",
                ));
                continue;
            }
            seen_here.push(machine.id.clone());
            if let Some(i) = out.config.machines.iter().position(|m| m.id == machine.id) {
                out.findings.push(finding(
                    Level::Info,
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
                out.machines[i] = MachineSource {
                    id: machine.id.clone(),
                    file: file.clone(),
                    config: cf.path.clone(),
                    state_dir: dir.join("state"),
                };
                out.config.machines[i] = machine;
                continue;
            }
            out.machines.push(MachineSource {
                id: machine.id.clone(),
                file: file.clone(),
                config: cf.path.clone(),
                state_dir: dir.join("state"),
            });
            out.config.machines.push(machine);
        }
    }
    out.config.idle = idle.unwrap_or_default();
    out
}

fn idle_prompt(
    findings: &mut Findings,
    config: &Path,
    dir: &Path,
    p: PromptToml,
    inline: bool,
) -> Vec<ActionDef> {
    match (p.file, p.text) {
        (Some(f), None) => {
            let full = dir.join(&f);
            match std::fs::read_to_string(&full) {
                Ok(t) => {
                    fence_warning(findings, config, &t);
                    if inline {
                        vec![ActionDef::Prompt(Prompt::Text(t))]
                    } else {
                        vec![ActionDef::Prompt(Prompt::File(full.display().to_string()))]
                    }
                }
                Err(e) => {
                    findings.push(finding(
                        Level::Error,
                        config,
                        None,
                        format!("idle on-enter file {f}: {e}"),
                        "CLI-3",
                    ));
                    Vec::new()
                }
            }
        }
        (None, Some(t)) => {
            fence_warning(findings, config, &t);
            vec![ActionDef::Prompt(Prompt::Text(t))]
        }
        _ => {
            findings.push(finding(
                Level::Error,
                config,
                None,
                "idle on-enter needs exactly one of file, text".to_string(),
                "CLI-3",
            ));
            Vec::new()
        }
    }
}

/// Idle text containing smllm's fences (TURN-12).
fn fence_warning(findings: &mut Findings, config: &Path, text: &str) {
    for f in ["</smllm>", "</instructions>", "</events>"] {
        if text.contains(f) {
            findings.push(finding(
                Level::Warning,
                config,
                None,
                format!("idle on-enter text contains `{f}`, which smllm uses to fence agent text"),
                "TURN-12",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_messages_lose_the_location_prefix_and_gain_hints() {
        let (m, h) = parse_message(
            "error: line 6 column 5: unknown field `states`, expected one of a\n --> x",
        );
        assert_eq!(m, "unknown field `states`, expected one of a");
        assert!(h.unwrap().contains("not in smllm v1"));
        let (m, h) = parse_message("plain");
        assert_eq!(m, "plain");
        assert!(h.is_none());
        let (m, _) = parse_message(
            "error: line 3 column 1: duplicate mapping key: initial, set DuplicateKeyPolicy in Options if acceptable",
        );
        assert_eq!(m, "duplicate key `initial`");
    }
}
