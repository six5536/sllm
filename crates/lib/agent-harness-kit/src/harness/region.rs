// Derived from sokf 9c93f37 crates/lib/sokf-core/src/harness/region.rs
//! The `region` kind: the tool owns one block between its markers in the
//! user's file.

use crate::hash::normalise;

/// A tool's region markers: `<!-- <tool>:harness -->` and
/// `<!-- /<tool>:harness -->`, each on a line of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Markers {
    /// The opening marker.
    pub open: String,
    /// The closing marker.
    pub close: String,
}

impl Markers {
    /// The markers of the tool named `tool`.
    pub fn new(tool: &str) -> Self {
        Markers {
            open: format!("<!-- {tool}:harness -->"),
            close: format!("<!-- /{tool}:harness -->"),
        }
    }

    /// The line indexes of the markers in LF text: the first opening line
    /// and the first closing line after it.
    fn find(&self, lines: &[&str]) -> Option<(usize, usize)> {
        let open = lines.iter().position(|l| l.trim_end() == self.open)?;
        let close = lines[open + 1..]
            .iter()
            .position(|l| l.trim_end() == self.close)?
            + open
            + 1;
        Some((open, close))
    }

    /// The block with its markers, LF, a blank line after the opening
    /// marker.
    fn framed(&self, block: &str) -> String {
        let mut body = strip_leading_blank_lines(&normalise(block));
        if !body.is_empty() && !body.ends_with('\n') {
            body.push('\n');
        }
        format!("{}\n\n{body}{}\n", self.open, self.close)
    }
}

/// LF text without its leading whitespace-only lines.
fn strip_leading_blank_lines(text: &str) -> String {
    let mut rest = text;
    while let Some(i) = rest.find('\n') {
        if !rest[..i].trim().is_empty() {
            break;
        }
        rest = &rest[i + 1..];
    }
    if rest.trim().is_empty() {
        String::new()
    } else {
        rest.to_string()
    }
}

/// The block between the markers, LF, without the blank lines that open
/// it and with its trailing newline; `None` when the markers are absent.
pub fn find_region(text: &str, markers: &Markers) -> Option<String> {
    let text = normalise(text);
    let lines: Vec<&str> = text.split('\n').collect();
    let (open, close) = markers.find(&lines)?;
    let inner = &lines[open + 1..close];
    if inner.is_empty() {
        return Some(String::new());
    }
    Some(strip_leading_blank_lines(&format!(
        "{}\n",
        inner.join("\n")
    )))
}

/// The file to write: the block rewritten between the first markers,
/// appended after one blank line when they are absent, or alone for an
/// absent or empty file. The file's line endings are kept. `None` when
/// nothing changes.
pub fn render_region(existing: Option<&str>, block: &str, markers: &Markers) -> Option<String> {
    let new_block = markers.framed(block);
    let Some(existing) = existing else {
        return Some(new_block);
    };
    let crlf = existing.contains("\r\n");
    let text = normalise(existing);
    let lines: Vec<&str> = text.split('\n').collect();
    let out = match markers.find(&lines) {
        Some((open, close)) => {
            let head = lines[..open].join("\n");
            let tail = lines[close + 1..].join("\n");
            let head = if open == 0 { head } else { format!("{head}\n") };
            format!("{head}{new_block}{tail}")
        }
        None if text.is_empty() => new_block,
        None => {
            let mut head = text.clone();
            if !head.ends_with('\n') {
                head.push('\n');
            }
            format!("{head}\n{new_block}")
        }
    };
    let out = if crlf { out.replace('\n', "\r\n") } else { out };
    (out != existing).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: &str = "Read the skill.\n";

    fn m() -> Markers {
        Markers::new("tool")
    }

    #[test]
    fn markers_carry_the_tool_name() {
        assert_eq!(m().open, "<!-- tool:harness -->");
        assert_eq!(m().close, "<!-- /tool:harness -->");
    }

    #[test]
    fn a_new_file_is_the_block_alone() {
        let out = render_region(None, BLOCK, &m()).unwrap();
        assert_eq!(
            out,
            "<!-- tool:harness -->\n\nRead the skill.\n<!-- /tool:harness -->\n"
        );
        assert_eq!(find_region(&out, &m()).as_deref(), Some(BLOCK));
        assert_eq!(render_region(Some(&out), BLOCK, &m()), None);
        // A block with no trailing newline gets one; an empty block is
        // the markers alone.
        assert_eq!(
            render_region(None, "x", &m()).unwrap(),
            "<!-- tool:harness -->\n\nx\n<!-- /tool:harness -->\n"
        );
        assert_eq!(
            render_region(None, "", &m()).unwrap(),
            "<!-- tool:harness -->\n\n<!-- /tool:harness -->\n"
        );
    }

    #[test]
    fn markers_absent_appends_after_one_blank_line() {
        let out = render_region(Some("# Title\n\nText."), BLOCK, &m()).unwrap();
        assert_eq!(
            out,
            "# Title\n\nText.\n\n<!-- tool:harness -->\n\nRead the skill.\n<!-- /tool:harness -->\n"
        );
        assert_eq!(
            render_region(Some(""), BLOCK, &m()).unwrap(),
            m().framed(BLOCK)
        );
        assert_eq!(
            render_region(Some("x\n"), BLOCK, &m()).unwrap(),
            format!("x\n\n{}", m().framed(BLOCK))
        );
    }

    #[test]
    fn markers_present_rewrites_between_the_first_ones_only() {
        let before =
            "# Title\n\n<!-- tool:harness -->\nold\n<!-- /tool:harness -->\n\n## After\n\nkept\n";
        let out = render_region(Some(before), BLOCK, &m()).unwrap();
        assert_eq!(
            out,
            "# Title\n\n<!-- tool:harness -->\n\nRead the skill.\n<!-- /tool:harness -->\n\n## After\n\nkept\n"
        );
        assert_eq!(render_region(Some(&out), BLOCK, &m()), None);
        let before = "<!-- tool:harness -->\n<!-- /tool:harness -->\ntail\n";
        assert_eq!(find_region(before, &m()).as_deref(), Some(""));
        assert_eq!(
            render_region(Some(before), BLOCK, &m()).unwrap(),
            "<!-- tool:harness -->\n\nRead the skill.\n<!-- /tool:harness -->\ntail\n"
        );
        // Another tool's markers are text like any other.
        let other = "<!-- sokf:harness -->\nx\n<!-- /sokf:harness -->\n";
        assert_eq!(find_region(other, &m()), None);
        assert!(
            render_region(Some(other), BLOCK, &m())
                .unwrap()
                .starts_with(other)
        );
    }

    #[test]
    fn keeps_crlf_line_endings() {
        let before = "a\r\n\r\n<!-- tool:harness -->\r\nold\r\n<!-- /tool:harness -->\r\n";
        let out = render_region(Some(before), BLOCK, &m()).unwrap();
        assert_eq!(
            out,
            "a\r\n\r\n<!-- tool:harness -->\r\n\r\nRead the skill.\r\n<!-- /tool:harness -->\r\n"
        );
        assert_eq!(find_region(&out, &m()).as_deref(), Some(BLOCK));
        assert_eq!(render_region(Some(&out), BLOCK, &m()), None);
        let out = render_region(Some("a\r\n"), BLOCK, &m()).unwrap();
        assert!(out.contains("\r\n<!-- tool:harness -->\r\n"), "{out:?}");
    }

    #[test]
    fn a_closing_marker_before_the_opening_one_is_no_block() {
        assert_eq!(
            find_region("<!-- /tool:harness -->\n<!-- tool:harness -->\n", &m()),
            None
        );
        assert_eq!(find_region("no markers", &m()), None);
    }
}
