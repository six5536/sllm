//! Header lines, quoting, timestamps and the agent rules text.
// @zen-component: TURN-Render

use crate::prelude::*;

/// The rules every agent needs (TURN-9); the MCP tool description and the
/// `AGENTS.md` block carry them (HOST-12).
// @zen-impl: TURN-9_AC-1
pub const AGENT_RULES: &str = "\
smllm runs state machines that guide your work. Everything smllm says is \
fenced in <smllm>…</smllm>: the state's instructions are in <instructions>, \
the events you may fire are in <events>.
- When you finish the work a state asks for, fire exactly one of its events \
with the `smllm` tool: smllm({ session, event, params }). The result is the \
next state's instructions.
- Pass the session key shown in every smllm header on every call.
- To stop and ask the user something, fire `yield` first, then end your turn.
- Call smllm with no event to see where you are and what you may fire.
- Only the main agent calls smllm. Never pass the session key to subagents.";

/// `session K · machine › STATE (visit n) · noun label` (TURN-1).
// @zen-impl: TURN-1_AC-1
pub(crate) fn header(
    key: &str,
    machine: &str,
    state: &str,
    visit: u32,
    noun: &str,
    label: &str,
) -> String {
    let mut h = format!("session {key} · {machine} › {state}");
    if visit >= 2 {
        h.push_str(&format!(" (visit {visit})"));
    }
    h.push_str(&format!(" · {noun} {label}"));
    h
}

/// `session K · idle`.
pub(crate) fn idle_header(key: &str) -> String {
    format!("session {key} · idle")
}

/// A param value in double quotes, inner quotes and newlines escaped.
pub(crate) fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `YYYY-MM-DD HH:MM UTC` from unix ms (no_std: no chrono).
pub fn format_utc(ms: u64) -> String {
    let secs = ms / 1000;
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        rem / 3600,
        (rem % 3600) / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_shows_visits_from_the_second() {
        assert_eq!(
            header("sm-1", "dev", "WORK", 1, "issue", "GH-1"),
            "session sm-1 · dev › WORK · issue GH-1"
        );
        assert_eq!(
            header("sm-1", "dev", "WORK", 3, "issue", "GH-1"),
            "session sm-1 · dev › WORK (visit 3) · issue GH-1"
        );
        assert_eq!(idle_header("sm-1"), "session sm-1 · idle");
    }

    #[test]
    fn quotes_escape() {
        assert_eq!(quote("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
    }

    #[test]
    fn utc_dates() {
        assert_eq!(format_utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_utc(951_782_400_000), "2000-02-29 00:00 UTC");
        assert_eq!(format_utc(1_790_332_320_000), "2026-09-25 10:32 UTC");
    }
}
