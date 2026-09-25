//! Collects findings for one machine file, with lines from YAML paths.

use std::path::{Path, PathBuf};

use crate::finding::{Finding, Findings, Level};
use crate::locate::locate;

/// Findings for one file.
pub(crate) struct Checker<'a> {
    pub file: &'a Path,
    pub text: &'a str,
    pub findings: Findings,
}

impl<'a> Checker<'a> {
    pub(crate) fn new(file: &'a Path, text: &'a str) -> Self {
        Self {
            file,
            text,
            findings: Findings::default(),
        }
    }

    pub(crate) fn add(
        &mut self,
        level: Level,
        path: &[String],
        message: String,
        hint: Option<&str>,
        rule: &'static str,
    ) {
        let segs: Vec<&str> = path.iter().map(String::as_str).collect();
        let line = locate(self.text, &segs);
        let shown = if path.is_empty() {
            None
        } else {
            let mut s = String::new();
            for seg in path {
                if !seg.starts_with('[') && !s.is_empty() {
                    s.push('.');
                }
                s.push_str(seg);
            }
            Some(s)
        };
        self.findings.push(Finding {
            level,
            file: PathBuf::from(self.file),
            line,
            path: shown,
            message,
            hint: hint.map(ToString::to_string),
            rule,
        });
    }

    pub(crate) fn error(
        &mut self,
        path: &[String],
        message: String,
        hint: Option<&str>,
        rule: &'static str,
    ) {
        self.add(Level::Error, path, message, hint, rule);
    }

    pub(crate) fn warning(
        &mut self,
        path: &[String],
        message: String,
        hint: Option<&str>,
        rule: &'static str,
    ) {
        self.add(Level::Warning, path, message, hint, rule);
    }
}

/// Build a path from segments.
#[macro_export]
#[doc(hidden)]
macro_rules! ypath {
    ($($seg:expr),* $(,)?) => {{
        let v: Vec<String> = [$($seg.to_string()),*].into();
        v
    }};
}
