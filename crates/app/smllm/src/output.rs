//! Output: stdout text or one JSON object, findings as a report (CLI
//! conventions).

use std::path::Path;

use agent_harness_kit::cli::{json_line, write_stdout};
use agent_harness_kit::report::Report;
use serde::Serialize;
use smllm_format::Findings;

use crate::error::{Error, Result};

pub use agent_harness_kit::cli::{EXIT_ERRORS, EXIT_OK};

/// A stdout write failure.
pub fn stdout_err(e: std::io::Error) -> Error {
    Error::io(Path::new("<stdout>"), e)
}

/// Print text as is.
pub fn text(s: &str) -> Result<()> {
    write_stdout(s.as_bytes()).map_err(stdout_err)
}

/// Print an engine reply: as JSON with `json`, else its text.
pub fn reply(reply: &smllm_core::Reply, json: bool) -> Result<()> {
    if json {
        self::json(reply)
    } else {
        text(&reply.text)
    }
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

/// smllm-format findings as a report, files shown relative to the cwd.
pub fn report(findings: &Findings) -> Report {
    findings.report(shown)
}
