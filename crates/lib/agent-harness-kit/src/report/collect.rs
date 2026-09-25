// Derived from sokf 9c93f37 crates/lib/sokf-core/src/report/collect.rs
//! `Report`: the findings of a run, ordered and serialised.

use serde::{
    Serialize,
    ser::{SerializeSeq, SerializeStruct},
};

use crate::report::{Finding, Severity};

/// Errors, warnings and info.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// The errors, ordered by path then line after `finish`.
    pub errors: Vec<Finding>,
    /// The warnings, ordered likewise.
    pub warnings: Vec<Finding>,
    /// The info findings, ordered likewise.
    pub info: Vec<Finding>,
}

impl Report {
    /// Add a finding to the list its severity selects.
    pub fn push(&mut self, finding: Finding) {
        match finding.severity {
            Severity::Error => self.errors.push(finding),
            Severity::Warning => self.warnings.push(finding),
            Severity::Info => self.info.push(finding),
        }
    }

    /// Add every finding.
    pub fn extend(&mut self, findings: impl IntoIterator<Item = Finding>) {
        for f in findings {
            self.push(f);
        }
    }

    /// Order findings by path, line then message, and remove duplicates.
    pub fn finish(mut self) -> Self {
        for list in [&mut self.errors, &mut self.warnings, &mut self.info] {
            list.sort_by(|a, b| (&a.path, a.line, &a.message).cmp(&(&b.path, b.line, &b.message)));
            list.dedup();
        }
        self
    }

    /// Whether the run found an error.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// One finding in the JSON form: `path`, `line` when there is one,
/// `message`, and `authority` when there is one.
struct Entry<'a>(&'a Finding);

impl Serialize for Entry<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("path", &self.0.path)?;
        if let Some(line) = self.0.line {
            m.serialize_entry("line", &line)?;
        }
        m.serialize_entry("message", &self.0.message)?;
        if !self.0.authority.is_empty() {
            m.serialize_entry("authority", &self.0.authority)?;
        }
        m.end()
    }
}

struct Entries<'a>(&'a [Finding]);

impl Serialize for Entries<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(self.0.len()))?;
        for f in self.0 {
            seq.serialize_element(&Entry(f))?;
        }
        seq.end()
    }
}

/// `{"errors": [...], "warnings": [...], "info": [...]}`.
impl Serialize for Report {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("Report", 3)?;
        st.serialize_field("errors", &Entries(&self.errors))?;
        st.serialize_field("warnings", &Entries(&self.warnings))?;
        st.serialize_field("info", &Entries(&self.info))?;
        st.end()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn json_has_three_lists() {
        let mut r = Report::default();
        r.push(Finding::error("a.yaml", Some(41), "x", "CFG-1"));
        r.push(Finding::warning("b.yaml", None, "w", ""));
        r.push(Finding::info("c.toml", None, "i", "CLI-3"));
        let r = r.finish();
        assert_eq!(
            serde_json::to_value(&r).unwrap(),
            serde_json::json!({
                "errors": [{"path": "a.yaml", "line": 41, "message": "x", "authority": "CFG-1"}],
                "warnings": [{"path": "b.yaml", "message": "w"}],
                "info": [{"path": "c.toml", "message": "i", "authority": "CLI-3"}]
            })
        );
        assert!(r.has_errors());
        assert!(!Report::default().has_errors());
    }

    fn arb_finding() -> impl Strategy<Value = Finding> {
        (
            prop::sample::select(vec!["a.md", "b/c.md", "b/a.md", "z.md"]),
            prop::option::of(1usize..50),
            prop::sample::select(vec!["m1", "m2"]),
            prop::sample::select(vec![Severity::Error, Severity::Warning, Severity::Info]),
        )
            .prop_map(|(path, line, msg, severity)| Finding::new(path, line, severity, msg, "A-1"))
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn finish_orders_by_path_then_line_and_dedups(findings in prop::collection::vec(arb_finding(), 0..12)) {
            let mut r = Report::default();
            r.extend(findings.clone());
            let r = r.finish();
            for list in [&r.errors, &r.warnings, &r.info] {
                for w in list.windows(2) {
                    prop_assert!((&w[0].path, w[0].line) <= (&w[1].path, w[1].line));
                    prop_assert_ne!(&w[0], &w[1]);
                }
            }
            prop_assert!(r.errors.iter().all(|f| f.severity == Severity::Error));
            prop_assert!(r.warnings.iter().all(|f| f.severity == Severity::Warning));
            prop_assert!(r.info.iter().all(|f| f.severity == Severity::Info));
            let mut expected = findings.clone();
            expected.sort();
            expected.dedup();
            prop_assert_eq!(r.errors.len() + r.warnings.len() + r.info.len(), expected.len());
        }

        #[test]
        fn json_shape(findings in prop::collection::vec(arb_finding(), 0..6)) {
            let mut r = Report::default();
            r.extend(findings);
            let text = serde_json::to_string(&r).unwrap();
            let e = text.find("\"errors\":").unwrap();
            let w = text.find("\"warnings\":").unwrap();
            let i = text.find("\"info\":").unwrap();
            prop_assert!(e < w && w < i, "{text}");
            let json: serde_json::Value = serde_json::from_str(&text).unwrap();
            let obj = json.as_object().unwrap();
            prop_assert_eq!(obj.len(), 3);
            for list in obj.values() {
                for entry in list.as_array().unwrap() {
                    let e = entry.as_object().unwrap();
                    prop_assert!(e.contains_key("path") && e.contains_key("message"));
                    prop_assert!(e.keys().all(|k| ["path", "line", "message", "authority"].contains(&k.as_str())));
                }
            }
        }
    }
}
