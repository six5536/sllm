//! Findings: every problem collected, with file, line, YAML path, level and
//! the rule it comes from (CFG-14).
// @zen-component: CFG-Findings

use std::path::PathBuf;

use serde::Serialize;

/// Finding level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Must be fixed; the config does not load.
    Error,
    /// Probably a mistake.
    Warning,
    /// Worth knowing.
    Info,
}

impl Level {
    /// Lower-case name.
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warning => "warning",
            Level::Info => "info",
        }
    }
}

/// One finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Level.
    pub level: Level,
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
    /// `<file>:<line>: <level>: <message> (<rule>)`, the hint on the next line.
    pub fn render(&self) -> String {
        let mut s = self.file.display().to_string();
        if let Some(l) = self.line {
            s.push_str(&format!(":{l}"));
        }
        s.push_str(&format!(": {}: ", self.level.as_str()));
        if let Some(p) = &self.path {
            s.push_str(&format!("{p}: "));
        }
        s.push_str(&format!("{} ({})", self.message, self.rule));
        if let Some(h) = &self.hint {
            s.push_str(&format!("\n  hint: {h}"));
        }
        s
    }
}

/// Collected findings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Findings(pub Vec<Finding>);

impl Findings {
    /// Whether any is an error.
    pub fn has_errors(&self) -> bool {
        self.count(Level::Error) > 0
    }

    /// How many at `level`.
    pub fn count(&self, level: Level) -> usize {
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
}
