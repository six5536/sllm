//! `smllm statusline`: where a session is, as a coloured row or JSON, for a
//! harness status line (STL). Never fails loudly: a non-zero exit would blank
//! Claude Code's whole status line.
// @zen-component: STL-Command

use std::io::{IsTerminal as _, Read as _};

use agent_harness_kit::hook::HookInput;
use smllm_core::{Error as EngineError, SessionStatus};

use crate::cli::{ColorChoice, StatuslineArgs};
use crate::commands::harness::bound_key;
use crate::error::{Error, Result};
use crate::output::{self, EXIT_OK};
use crate::runtime::Runtime;

/// `smllm statusline`: the row, `--json` object, or nothing; always exit 0.
// @zen-impl: STL-1_AC-1
// @zen-impl: STL-4_AC-1
pub fn statusline(args: &StatuslineArgs) -> Result<u8> {
    let status = current(args.session.as_deref()).unwrap_or_else(|e| {
        eprintln!("smllm statusline: {e}");
        None
    });
    if args.json {
        match &status {
            Some(s) => output::json(s)?,
            None => output::text("{}\n")?,
        }
    } else if let Some(s) = &status {
        let no_color = std::env::var("NO_COLOR").ok();
        output::text(&row(s, use_color(args.color, no_color.as_deref())))?;
    }
    Ok(EXIT_OK)
}

/// The session to show: `--session`, else the one bound to the status JSON's
/// `session_id` on stdin; `None` when there is none (STL-2).
// @zen-impl: STL-2_AC-1
// @zen-impl: STL-2_AC-2
fn current(session: Option<&str>) -> Result<Option<SessionStatus>> {
    let key = match session {
        Some(k) => k.to_string(),
        None => {
            let mut stdin = String::new();
            // Run by hand without a pipe: nothing to read, nothing to show.
            if !std::io::stdin().is_terminal() {
                let _ = std::io::stdin().read_to_string(&mut stdin);
            }
            match bound_key(HookInput::parse(&stdin).session_id.as_deref())? {
                Some(k) => k,
                None => return Ok(None),
            }
        }
    };
    let mut rt = Runtime::for_session(&key)?;
    match rt.with(|e, h| e.status(h, &key)) {
        Ok(s) => Ok(Some(s)),
        Err(EngineError::UnknownSession(_)) => Ok(None),
        Err(e) => Err(Error::from(e)),
    }
}

/// Whether to colour the row: `--color`, else off when `NO_COLOR` is set and
/// non-empty, else on (a status line never has a TTY to check).
// @zen-component: STL-Row
// @zen-impl: STL-7_AC-1
pub(crate) fn use_color(flag: Option<ColorChoice>, no_color: Option<&str>) -> bool {
    match flag {
        Some(ColorChoice::Always) => true,
        Some(ColorChoice::Never) => false,
        Some(ColorChoice::Auto) | None => no_color.is_none_or(str::is_empty),
    }
}

/// The default row, without a trailing newline (STL-6).
// @zen-impl: STL-6_AC-1
// @zen-impl: STL-6_AC-2
// @zen-impl: STL-6_AC-3
pub(crate) fn row(s: &SessionStatus, color: bool) -> String {
    let paint = |sgr: &str, text: &str| {
        if color {
            format!("\x1b[{sgr}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    };
    let (dim, cyan, bold, magenta, yellow) = ("2", "36", "1", "35", "33");
    let mut out = paint(dim, "smllm");
    match (&s.machine, &s.state, &s.instance) {
        (Some(machine), Some(state), Some(inst)) => {
            out.push_str(&format!(
                " {} › {}",
                paint(cyan, machine),
                paint(bold, state)
            ));
            if let Some(n) = s.visit.filter(|n| *n >= 2) {
                out.push_str(&format!(" {}", paint(dim, &format!("(visit {n})"))));
            }
            out.push_str(&format!(
                " · {}",
                paint(magenta, &format!("{} {}", inst.kind, inst.label))
            ));
            if s.yielded {
                out.push_str(&format!(" · {}", paint(yellow, "yielded")));
            }
        }
        _ => {
            out.push_str(" idle");
            let suspended = u32::from(s.suspended.is_some());
            for (n, what) in [(suspended, "suspended"), (s.parked, "parked")] {
                if n > 0 {
                    out.push_str(&format!(" · {}", paint(dim, &format!("{n} {what}"))));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use smllm_core::InstanceStatus;

    use super::*;

    fn working(visit: u32, yielded: bool) -> SessionStatus {
        SessionStatus {
            session: "sm-k7f3q2".into(),
            idle: false,
            machine: Some("dev".into()),
            state: Some("WORK".into()),
            visit: Some(visit),
            yielded,
            instance: Some(InstanceStatus {
                machine: "dev".into(),
                kind: "issue".into(),
                id: "a1b2c3".into(),
                r#ref: Some("GH-123".into()),
                label: "GH-123".into(),
                status: "active".into(),
            }),
            suspended: None,
            parked: 2,
        }
    }

    fn idle(suspended: bool, parked: u32) -> SessionStatus {
        let mut s = working(1, false);
        let inst = s.instance.take().map(|mut i| {
            i.status = "suspended".into();
            i
        });
        SessionStatus {
            idle: true,
            machine: None,
            state: None,
            visit: None,
            suspended: inst.filter(|_| suspended),
            parked,
            ..s
        }
    }

    // @zen-test: STL-6_AC-1
    // @zen-test: STL-6_AC-2
    // @zen-test: STL-6_AC-3
    #[test]
    fn default_rows() {
        let rows = [
            row(&working(1, false), false),
            row(&working(3, false), false),
            row(&working(3, true), false),
            row(&idle(false, 0), false),
            row(&idle(true, 2), false),
            row(&idle(false, 1), false),
            row(&working(3, true), true),
            row(&idle(true, 2), true),
        ];
        insta::assert_snapshot!(rows.join("\n").replace('\x1b', "\\e"));
    }

    // @zen-test: STL-7_AC-1
    #[test]
    fn colour_choice() {
        assert!(use_color(None, None));
        assert!(use_color(None, Some("")));
        assert!(!use_color(None, Some("1")));
        assert!(!use_color(Some(ColorChoice::Auto), Some("1")));
        assert!(use_color(Some(ColorChoice::Always), Some("1")));
        assert!(!use_color(Some(ColorChoice::Never), None));
    }

    // @zen-test: STL-5_AC-1
    // @zen-test: STL-5_AC-2
    #[test]
    fn the_json_contract() {
        let json = |s: &SessionStatus| serde_json::to_string_pretty(s).unwrap();
        insta::assert_snapshot!(format!(
            "{}\n{}",
            json(&working(3, true)),
            json(&idle(true, 2))
        ));
    }
}
