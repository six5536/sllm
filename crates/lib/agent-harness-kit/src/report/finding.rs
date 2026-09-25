// Derived from sokf 9c93f37 crates/lib/sokf-core/src/report/finding.rs
//! `Finding`: one line of a report.

use serde::{Deserialize, Serialize};

/// How a finding is reported.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// A failure. Exit 1.
    #[default]
    Error,
    /// What the tool cannot settle alone.
    Warning,
    /// Worth knowing; never a failure.
    Info,
}

impl Severity {
    /// The level as the text report shows it.
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

/// One line of the report.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Finding {
    /// The file the finding is on, as the tool shows paths.
    pub path: String,
    /// The line, when one applies.
    pub line: Option<usize>,
    /// Error, warning or info.
    pub severity: Severity,
    /// The message.
    pub message: String,
    /// What the finding breaks, e.g. `CFG-3` or `spec §8`; shown in
    /// parentheses after the message. Empty for none.
    pub authority: String,
}

impl Finding {
    /// Build a finding.
    pub fn new(
        path: impl Into<String>,
        line: Option<usize>,
        severity: Severity,
        message: impl Into<String>,
        authority: impl Into<String>,
    ) -> Self {
        Finding {
            path: path.into(),
            line,
            severity,
            message: message.into(),
            authority: authority.into(),
        }
    }

    /// An error finding.
    pub fn error(
        path: impl Into<String>,
        line: Option<usize>,
        message: impl Into<String>,
        authority: impl Into<String>,
    ) -> Self {
        Self::new(path, line, Severity::Error, message, authority)
    }

    /// A warning finding.
    pub fn warning(
        path: impl Into<String>,
        line: Option<usize>,
        message: impl Into<String>,
        authority: impl Into<String>,
    ) -> Self {
        Self::new(path, line, Severity::Warning, message, authority)
    }

    /// An info finding.
    pub fn info(
        path: impl Into<String>,
        line: Option<usize>,
        message: impl Into<String>,
        authority: impl Into<String>,
    ) -> Self {
        Self::new(path, line, Severity::Info, message, authority)
    }

    /// The message with its authority: `<message> (<authority>)`.
    pub fn full_message(&self) -> String {
        if self.authority.is_empty() {
            self.message.clone()
        } else {
            format!("{} ({})", self.message, self.authority)
        }
    }

    /// The text line: `<path>:<line>: <level>: <message> (<authority>)`,
    /// without `:<line>` when there is none.
    pub fn to_line(&self) -> String {
        let level = self.severity.as_str();
        match self.line {
            Some(line) => format!("{}:{line}: {level}: {}", self.path, self.full_message()),
            None => format!("{}: {level}: {}", self.path, self.full_message()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_carries_the_level_and_authority() {
        let f = Finding::error("a.yaml", Some(3), "bad", "CFG-1");
        assert_eq!(f.to_line(), "a.yaml:3: error: bad (CFG-1)");
        let f = Finding::warning("a.yaml", None, "odd", "");
        assert_eq!(f.to_line(), "a.yaml: warning: odd");
        let f = Finding::info("c.toml", Some(1), "project wins", "CLI-3");
        assert_eq!(f.severity, Severity::Info);
        assert_eq!(f.to_line(), "c.toml:1: info: project wins (CLI-3)");
    }

    #[test]
    fn findings_round_trip_through_serde() {
        let f = Finding::error("a.md", Some(3), "bad", "spec §8");
        let json = serde_json::to_string(&f).unwrap();
        assert!(json.contains("\"severity\":\"error\""), "{json}");
        let back: Finding = serde_json::from_str(&json).unwrap();
        assert_eq!(back, f);
    }
}
