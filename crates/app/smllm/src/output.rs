//! Output: stdout text or one JSON object, findings as a report (CLI
//! conventions).

use std::path::Path;

use agent_harness_kit::cli::{json_line, write_stdout};
use agent_harness_kit::report::{Finding, Report, Severity};
use serde::Serialize;
use smllm_format::{Findings, Level};

use crate::error::{Error, Result};

pub use agent_harness_kit::cli::{EXIT_ERRORS, EXIT_FAILURE, EXIT_OK};

fn stdout_err(e: std::io::Error) -> Error {
    Error::io(Path::new("<stdout>"), e)
}

/// Print text as is.
pub fn text(s: &str) -> Result<()> {
    write_stdout(s.as_bytes()).map_err(stdout_err)
}

/// Print one JSON object and a newline.
pub fn json(v: &impl Serialize) -> Result<()> {
    let buf = json_line(v).map_err(|e| Error::msg(e.to_string()))?;
    write_stdout(&buf).map_err(stdout_err)
}

/// A path relative to the working dir when below it.
pub fn shown(path: &Path) -> String {
    let cwd = std::env::current_dir().unwrap_or_default();
    path.strip_prefix(&cwd)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// smllm-format findings as a report (YAML path and hint folded into the
/// message).
pub fn report(findings: &Findings) -> Report {
    let mut r = Report::default();
    for f in &findings.0 {
        let mut msg = String::new();
        if let Some(p) = &f.path {
            msg.push_str(p);
            msg.push_str(": ");
        }
        msg.push_str(&f.message);
        if let Some(h) = &f.hint {
            msg.push_str(&format!(" — {h}"));
        }
        let severity = match f.level {
            Level::Error => Severity::Error,
            Level::Warning => Severity::Warning,
            Level::Info => Severity::Info,
        };
        r.push(Finding::new(shown(&f.file), f.line, severity, msg, f.rule));
    }
    r.finish()
}
