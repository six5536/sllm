// Derived from sokf 9c93f37 crates/lib/sokf-core/src/harness/hook/claude.rs
//! A Claude Code hook's input and answer, and the helper that writes the
//! answer: JSON on stdout and exit 0, or on failure exit 1 with the message
//! on stderr and nothing on stdout, so a hook never wedges the agent.

use std::{fmt::Display, io::Write};

use serde::Deserialize;
use serde_json::json;

use crate::cli::{EXIT_ERRORS, EXIT_OK};

/// The fields of Claude Code's hook input the kit knows. Every field is
/// optional; unknown fields are ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct HookInput {
    /// The harness's session id.
    pub session_id: Option<String>,
    /// The transcript file.
    pub transcript_path: Option<String>,
    /// The agent's working directory.
    pub cwd: Option<String>,
    /// The event, e.g. `Stop`, `SessionStart`, `UserPromptSubmit`.
    pub hook_event_name: Option<String>,
    /// `SessionStart`: `startup`, `resume`, `clear` or `compact`.
    pub source: Option<String>,
    /// `UserPromptSubmit`: the user's prompt.
    pub prompt: Option<String>,
    /// `Stop`: whether the agent already continues because of a stop hook.
    pub stop_hook_active: Option<bool>,
}

impl HookInput {
    /// Parse the event object from stdin. Input that does not parse is
    /// treated as empty, since no field is required.
    pub fn parse(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }
}

/// A hook's answer to Claude Code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// Nothing to say: `{}`. For `Stop`, the agent may stop. `stderr`
    /// carries text for the user, e.g. a report the hook already blocked on.
    Allow {
        /// Text for stderr, when there is one.
        stderr: Option<String>,
    },
    /// `Stop`: the agent continues with the reason as its input:
    /// `{"decision": "block", "reason": …}`.
    Block {
        /// The reason.
        reason: String,
    },
    /// `SessionStart` / `UserPromptSubmit`: text added to the agent's
    /// context: `{"hookSpecificOutput": {"hookEventName": …,
    /// "additionalContext": …}}`.
    Context {
        /// The event name, e.g. `SessionStart`.
        event: String,
        /// The text.
        context: String,
    },
}

impl Answer {
    /// The JSON object Claude Code reads on stdout.
    pub fn to_json(&self) -> String {
        match self {
            Answer::Allow { .. } => json!({}).to_string(),
            Answer::Block { reason } => {
                json!({ "decision": "block", "reason": reason }).to_string()
            }
            Answer::Context { event, context } => json!({
                "hookSpecificOutput": {
                    "hookEventName": event,
                    "additionalContext": context,
                }
            })
            .to_string(),
        }
    }

    /// The text for stderr, when there is one.
    pub fn stderr(&self) -> Option<&str> {
        match self {
            Answer::Allow { stderr } => stderr.as_deref(),
            _ => None,
        }
    }
}

/// Write a hook's outcome and return the exit code. An answer: its stderr
/// text, the JSON and a newline on stdout, exit 0. A failure: `error:
/// <message>` on stderr, nothing on stdout, exit 1.
pub fn emit<E: Display>(
    result: Result<Answer, E>,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> std::io::Result<u8> {
    match result {
        Ok(answer) => {
            if let Some(text) = answer.stderr() {
                stderr.write_all(text.as_bytes())?;
            }
            stdout.write_all(format!("{}\n", answer.to_json()).as_bytes())?;
            stdout.flush()?;
            Ok(EXIT_OK)
        }
        Err(e) => {
            writeln!(stderr, "error: {e}")?;
            Ok(EXIT_ERRORS)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_answer_as_json() {
        assert_eq!(Answer::Allow { stderr: None }.to_json(), "{}");
        assert_eq!(
            Answer::Allow {
                stderr: Some("x".into())
            }
            .to_json(),
            "{}"
        );
        assert_eq!(
            Answer::Block {
                reason: "t: 1 error remains".into()
            }
            .to_json(),
            "{\"decision\":\"block\",\"reason\":\"t: 1 error remains\"}"
        );
        assert_eq!(
            Answer::Context {
                event: "SessionStart".into(),
                context: "hi".into()
            }
            .to_json(),
            "{\"hookSpecificOutput\":{\"hookEventName\":\"SessionStart\",\"additionalContext\":\"hi\"}}"
        );
        assert_eq!(Answer::Block { reason: "r".into() }.stderr(), None);
    }

    #[test]
    fn input_is_lenient() {
        let i = HookInput::parse(
            r#"{"session_id":"s1","cwd":"/p","hook_event_name":"Stop","stop_hook_active":true,"other":1}"#,
        );
        assert_eq!(i.session_id.as_deref(), Some("s1"));
        assert_eq!(i.cwd.as_deref(), Some("/p"));
        assert_eq!(i.hook_event_name.as_deref(), Some("Stop"));
        assert_eq!(i.stop_hook_active, Some(true));
        assert_eq!(HookInput::parse("not json"), HookInput::default());
        let i = HookInput::parse(r#"{"source":"clear","prompt":"p","transcript_path":"/t"}"#);
        assert_eq!(
            (i.source.as_deref(), i.prompt.as_deref()),
            (Some("clear"), Some("p"))
        );
        assert_eq!(i.transcript_path.as_deref(), Some("/t"));
    }

    #[test]
    fn emit_writes_the_answer_or_the_error() {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = emit::<String>(
            Ok(Answer::Allow {
                stderr: Some("report\n".into()),
            }),
            &mut out,
            &mut err,
        )
        .unwrap();
        assert_eq!(
            (code, out.as_slice(), err.as_slice()),
            (0, &b"{}\n"[..], &b"report\n"[..])
        );
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = emit(Err::<Answer, _>("no config"), &mut out, &mut err).unwrap();
        assert_eq!(code, 1);
        assert!(out.is_empty());
        assert_eq!(String::from_utf8(err).unwrap(), "error: no config\n");
    }
}
