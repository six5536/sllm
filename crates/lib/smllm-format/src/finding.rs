//! Findings: every problem collected, with file, line, YAML path, level and
//! the rule it comes from (CFG-14).
// @zen-component: CFG-Findings

use std::path::{Path, PathBuf};

pub use agent_harness_kit::report::Severity;
use agent_harness_kit::report::{self, Report};
use serde::Serialize;

/// One finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Severity.
    pub level: Severity,
    /// File.
    pub file: PathBuf,
    /// 1-based line, when known.
    pub line: Option<usize>,
    /// YAML path (`states.WORK.on.submit[0]`), when relevant.
    pub path: Option<String>,
    /// What is wrong.
    pub message: String,
    /// How to fix it.
    pub hint: Option<String>,
    /// The rule (requirement id).
    pub rule: &'static str,
}

impl Finding {
    /// The report line users see: the YAML path and hint folded into the
    /// message; `shown` renders the file.
    pub fn to_report(&self, shown: &str) -> report::Finding {
        let mut msg = String::new();
        if let Some(p) = &self.path {
            msg.push_str(p);
            msg.push_str(": ");
        }
        msg.push_str(&self.message);
        if let Some(h) = &self.hint {
            msg.push_str(&format!(" — {h}"));
        }
        report::Finding::new(shown, self.line, self.level, msg, self.rule)
    }
}

/// Collected findings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Findings(pub Vec<Finding>);

impl Findings {
    /// Whether any is an error.
    pub fn has_errors(&self) -> bool {
        self.count(Severity::Error) > 0
    }

    /// How many at `level`.
    pub fn count(&self, level: Severity) -> usize {
        self.0.iter().filter(|f| f.level == level).count()
    }

    /// Add one.
    pub fn push(&mut self, f: Finding) {
        self.0.push(f);
    }

    /// Add all of another set.
    pub fn extend(&mut self, other: Findings) {
        self.0.extend(other.0);
    }

    /// As the kit's report (what `validate` prints); `shown` renders each
    /// file.
    pub fn report(&self, shown: impl Fn(&Path) -> String) -> Report {
        let mut r = Report::default();
        for f in &self.0 {
            r.push(f.to_report(&shown(&f.file)));
        }
        r.finish()
    }
}
