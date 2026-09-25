//! `harness install|status|hook` for Claude Code (HOST-6..11, CLI-7, CLI-8).
// @zen-component: HOST-Claude

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use agent_harness_kit::hook::{Answer, HookInput, emit};
use agent_harness_kit::{
    DeclinedStore, ExternalPart, InstallOptions, MergeOp, Part, Profile, Scope, TomlDeclined, Tool,
    install, status,
};
use serde_json::json;
use smllm_core::Stop;
use smllm_core::host::Store as _;

use crate::cli::{HarnessArgs, HookArgs};
use crate::error::{Error, Result};
use crate::output::{self, EXIT_ERRORS, EXIT_OK};
use crate::paths::{self, CONFIG_FILE, PROJECT_DIR};
use crate::runtime::Runtime;
use crate::store::FsStore;

/// The tiny orientation block for `AGENTS.md` / `CLAUDE.md` (D29); the full
/// rules are in the MCP tool description.
// @zen-impl: HOST-11_AC-1
pub const INSTRUCTIONS_BLOCK: &str = "\
## smllm

This project's work is guided by smllm state machines. Everything smllm says is fenced in \
`<smllm>…</smllm>`: follow its `<instructions>`, and when the work is done fire one of its \
`<events>` with the `smllm` MCP tool, passing the session key from the `<smllm>` header. \
Fire `yield` before stopping to ask the user something. Only the main agent calls smllm.
";

const HOOK_PREFIX: &str = "smllm harness hook ";
const HOOKS: [(&str, &str); 3] = [
    ("SessionStart", "session-start"),
    ("UserPromptSubmit", "user-prompt-submit"),
    ("Stop", "stop"),
];

fn server() -> serde_json::Value {
    json!({ "command": "smllm", "args": ["mcp"] })
}

/// User-scope MCP registration through `claude mcp`, never by editing
/// `~/.claude.json` (HOST-6); that file is only read to observe it.
#[derive(Debug)]
struct ClaudeMcpUser {
    claude_json: PathBuf,
}

impl ExternalPart for ClaudeMcpUser {
    fn location(&self) -> String {
        "claude mcp (user)".into()
    }

    fn expected(&self) -> String {
        server().to_string()
    }

    fn observe(&self) -> agent_harness_kit::Result<Option<String>> {
        let Ok(text) = std::fs::read_to_string(&self.claude_json) else {
            return Ok(None);
        };
        let doc: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        Ok(doc.pointer("/mcpServers/smllm").map(|s| {
            json!({ "command": s.get("command").cloned().unwrap_or_default(), "args": s.get("args").cloned().unwrap_or_default() })
                .to_string()
        }))
    }

    fn write(&self) -> agent_harness_kit::Result<()> {
        let _ = Command::new("claude")
            .args(["mcp", "remove", "--scope", "user", "smllm"])
            .output();
        let out = Command::new("claude")
            .args([
                "mcp",
                "add-json",
                "--scope",
                "user",
                "smllm",
                &server().to_string(),
            ])
            .output()
            .map_err(|e| agent_harness_kit::Error::Harness(format!("could not run claude: {e}")))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(agent_harness_kit::Error::Harness(format!(
                "claude mcp add-json failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )))
        }
    }
}

/// smllm as a harness-kit tool.
pub struct Smllm {
    project: PathBuf,
    claude_user: PathBuf,
    user_config: PathBuf,
}

impl Smllm {
    /// The tool for a project: the dir holding the `.smllm/` of `--config` /
    /// SMLLM_CONFIG, else of the nearest project config, else `cwd`.
    pub fn new(cwd: &Path, explicit: Option<&Path>) -> Result<Self> {
        let config = paths::explicit(explicit)
            .map(|p| cwd.join(p))
            .or_else(|| paths::project_config(cwd));
        let project = config
            .and_then(|c| {
                let dir = c.parent()?;
                if dir.file_name().is_some_and(|n| n == PROJECT_DIR) {
                    dir.parent().map(Path::to_path_buf)
                } else {
                    Some(dir.to_path_buf())
                }
            })
            .unwrap_or_else(|| cwd.to_path_buf());
        Ok(Self {
            project,
            claude_user: paths::claude_user_dir()?,
            user_config: paths::user_config_dir()?,
        })
    }
}

impl Tool for Smllm {
    fn name(&self) -> &str {
        "smllm"
    }

    fn profile(&self, harness: &str, scope: Scope) -> Option<Profile> {
        if harness != "claude" {
            return None;
        }
        let settings = match scope {
            Scope::Project => ".claude/settings.json",
            Scope::User => "settings.json",
        };
        let mcp = match scope {
            Scope::Project => Part::merge(
                "mcp",
                ".mcp.json",
                vec![MergeOp::object_member("mcpServers", "smllm", server())],
            ),
            Scope::User => Part::external(
                "mcp",
                Arc::new(ClaudeMcpUser {
                    claude_json: self.claude_user.with_file_name(".claude.json"),
                }),
            ),
        };
        let hooks = HOOKS
            .iter()
            .map(|(event, hook)| {
                MergeOp::hook_command(
                    *event,
                    HOOK_PREFIX,
                    &format!("smllm harness hook claude {hook}"),
                )
            })
            .collect();
        Some(Profile {
            harness: "claude".into(),
            parts: vec![
                Part::instructions("instructions", INSTRUCTIONS_BLOCK),
                mcp,
                Part::merge("hooks", settings, hooks),
                Part::merge(
                    "permissions",
                    settings,
                    vec![MergeOp::array_entry(
                        "permissions.allow",
                        "mcp__smllm__smllm",
                    )],
                ),
            ],
            hooks: HOOKS.iter().map(|(_, h)| (*h).to_string()).collect(),
        })
    }

    fn root(&self, scope: Scope) -> agent_harness_kit::Result<PathBuf> {
        Ok(match scope {
            Scope::Project => self.project.clone(),
            Scope::User => self.claude_user.clone(),
        })
    }

    fn record_path(&self, scope: Scope) -> agent_harness_kit::Result<PathBuf> {
        Ok(match scope {
            Scope::Project => self.project.join(PROJECT_DIR).join("harness.toml"),
            Scope::User => self.user_config.join("harness.toml"),
        })
    }

    fn declined_store(
        &self,
        scope: Scope,
    ) -> agent_harness_kit::Result<Box<dyn DeclinedStore + '_>> {
        Ok(Box::new(match scope {
            Scope::Project => TomlDeclined::new(
                self.project.join(PROJECT_DIR).join(CONFIG_FILE),
                ".smllm/config.toml",
            ),
            Scope::User => TomlDeclined::new(
                self.user_config.join(CONFIG_FILE),
                "~/.config/smllm/config.toml",
            ),
        }))
    }
}

/// `smllm harness install|status`.
// @zen-impl: HOST-6_AC-1
// @zen-impl: CLI-7_AC-1
pub fn run(args: &HarnessArgs, installing: bool, explicit: Option<&Path>) -> Result<u8> {
    let cwd = std::env::current_dir().map_err(|e| Error::io(Path::new("."), e))?;
    let tool = Smllm::new(&cwd, explicit)?;
    let scope: Scope = args.scope.parse()?;
    let result = if installing {
        let without = if args.without.is_empty() {
            None
        } else {
            Some(args.without.clone())
        };
        install(
            &tool,
            &InstallOptions {
                harness: args.name.clone(),
                scope,
                without,
                force: args.force,
            },
        )?
    } else {
        status(&tool, &args.name, scope)?
    };
    if args.json {
        output::json(&result)?;
    } else {
        output::text(&result.to_text())?;
        if result.parts.iter().any(|p| p.state == "edited") && !args.force {
            eprintln!(
                "note: parts marked edited were changed by hand and left alone; \
                 `smllm harness install {} --force` overwrites them",
                args.name
            );
        }
    }
    Ok(EXIT_OK)
}

/// The session key bound to a Claude Code session id.
fn bound_key(session_id: Option<&str>) -> Result<Option<String>> {
    let Some(sid) = session_id else {
        return Ok(None);
    };
    let mut store = FsStore::new(paths::user_state_dir()?, Default::default());
    store
        .binding("claude", sid)
        .map_err(|e| Error::msg(e.to_string()))
}

/// One hook call: stdin JSON in, an answer out.
// @zen-impl: HOST-7_AC-1
// @zen-impl: HOST-8_AC-1
pub fn answer(hook: &str, input: &HookInput) -> Result<Answer> {
    let allow = Answer::Allow { stderr: None };
    let key = bound_key(input.session_id.as_deref())?;
    match hook {
        "session-start" => {
            if let Some(k) = &key {
                let mut rt = Runtime::for_session(k)?;
                if rt.store.session(k).ok().flatten().is_some() {
                    let reply = rt.with(|e, h| e.view(h, k))?;
                    return Ok(Answer::Context {
                        event: "SessionStart".into(),
                        context: reply.text,
                    });
                }
            }
            let cwd = match &input.cwd {
                Some(c) => PathBuf::from(c),
                None => std::env::current_dir().map_err(|e| Error::io(Path::new("."), e))?,
            };
            let files = paths::lookup(None, &cwd)?;
            if files.is_empty() {
                return Ok(allow);
            }
            let mut rt = Runtime::new(&files)?;
            let reply = rt.bind(
                "claude",
                input.session_id.as_deref(),
                &cwd.display().to_string(),
            )?;
            Ok(Answer::Context {
                event: "SessionStart".into(),
                context: reply.text,
            })
        }
        "user-prompt-submit" => {
            if let Some(k) = &key {
                let mut rt = Runtime::for_session(k)?;
                rt.with(|e, h| e.prompt_submitted(h, k))?;
            }
            Ok(allow)
        }
        "stop" => {
            let Some(k) = &key else { return Ok(allow) };
            let mut rt = Runtime::for_session(k)?;
            let active = input.stop_hook_active.unwrap_or(false);
            Ok(match rt.with(|e, h| e.stop(h, k, active))? {
                Stop::Allow => allow,
                Stop::Block(reason) => Answer::Block { reason },
                Stop::Runaway(text) => Answer::Allow { stderr: Some(text) },
            })
        }
        other => Err(Error::msg(format!(
            "no hook named {other} (session-start, user-prompt-submit, stop)"
        ))),
    }
}

/// `smllm harness hook <NAME> <HOOK>`: failures exit 1 with stderr only.
// @zen-impl: CLI-8_AC-1
pub fn hook(args: &HookArgs) -> Result<u8> {
    if args.name != "claude" {
        eprintln!("error: no harness named {} (claude)", args.name);
        // A failed hook exits 1, stderr only (HOST-7).
        return Ok(EXIT_ERRORS);
    }
    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);
    let input = HookInput::parse(&stdin);
    let result = answer(&args.hook, &input);
    emit(
        result,
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    )
    .map_err(|e| Error::io(Path::new("<stdout>"), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    // @zen-test: HOST-11_AC-1
    // @zen-test: HOST-12_AC-1
    #[test]
    fn agent_facing_rules_are_a_stable_contract() {
        insta::assert_snapshot!("instructions_block", INSTRUCTIONS_BLOCK);
        insta::assert_snapshot!("tool_description", crate::commands::mcp::description());
    }
}
