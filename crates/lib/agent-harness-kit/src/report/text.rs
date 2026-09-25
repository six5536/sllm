// Derived from sokf 9c93f37 crates/lib/sokf-core/src/report/text.rs
//! The text report: findings, then the counts line.

use crate::report::Report;

/// The text report: the errors, the warnings when `warnings`, the info
/// findings when `info` (ordered together by path then line), one
/// `<path>:<line>: <level>: <message> (<authority>)` line each; then the
/// counts line, e.g. `1 error, 0 warnings, 2 info`.
pub fn report_text(report: &Report, warnings: bool, info: bool) -> String {
    let mut findings: Vec<_> = report.errors.iter().collect();
    if warnings {
        findings.extend(report.warnings.iter());
    }
    if info {
        findings.extend(report.info.iter());
    }
    findings.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
    let mut out = String::new();
    for f in findings {
        out.push_str(&f.to_line());
        out.push('\n');
    }
    out.push_str(&format!(
        "{}, {}, {} info\n",
        plural(report.errors.len(), "error"),
        plural(report.warnings.len(), "warning"),
        report.info.len()
    ));
    out
}

fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Finding;

    fn report() -> Report {
        let mut r = Report::default();
        r.push(Finding::error("b.yaml", Some(2), "bad", "CFG-1"));
        r.push(Finding::warning("a.yaml", None, "odd", "CFG-2"));
        r.push(Finding::info("a.yaml", Some(1), "note", ""));
        r.finish()
    }

    #[test]
    fn errors_only_by_default() {
        insta::assert_snapshot!(report_text(&report(), false, false), @r"
        b.yaml:2: error: bad (CFG-1)
        1 error, 1 warning, 1 info
        ");
    }

    #[test]
    fn warnings_and_info_are_listed_in_path_order_when_asked() {
        insta::assert_snapshot!(report_text(&report(), true, true), @r"
        a.yaml: warning: odd (CFG-2)
        a.yaml:1: info: note
        b.yaml:2: error: bad (CFG-1)
        1 error, 1 warning, 1 info
        ");
        assert_eq!(
            report_text(&Report::default(), true, true),
            "0 errors, 0 warnings, 0 info\n"
        );
    }
}
