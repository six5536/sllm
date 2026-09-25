//! Find the line of a YAML path in block- or flow-style YAML, for findings
//! about well-formed files (serde reports its own locations).

/// The 1-based line where `path` (keys; `[n]` list items) is written, or the
/// deepest ancestor found.
pub fn locate(text: &str, path: &[&str]) -> Option<usize> {
    let lines: Vec<&str> = text.lines().collect();
    let mut line = 0usize; // search start
    let mut indent: isize = -1; // parent's indentation
    let mut found = None;
    for seg in path {
        let hit = if let Some(idx) = seg.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let n: usize = idx.parse().ok()?;
            find_item(&lines, line, indent, n)
        } else {
            find_key(&lines, line, indent, seg)
        };
        match hit {
            Some((l, ind)) => {
                found = Some(l + 1);
                line = l;
                indent = ind;
            }
            None => break,
        }
    }
    found
}

fn indent_of(l: &str) -> isize {
    (l.len() - l.trim_start().len()) as isize
}

fn is_blank(l: &str) -> bool {
    let t = l.trim();
    t.is_empty() || t.starts_with('#')
}

/// A `key:` below the parent at `start` (same line counts, for flow maps).
fn find_key(lines: &[&str], start: usize, parent: isize, key: &str) -> Option<(usize, isize)> {
    let needles = [
        format!("{key}:"),
        format!("\"{key}\":"),
        format!("'{key}':"),
    ];
    for (i, l) in lines.iter().enumerate().skip(start) {
        if is_blank(l) {
            continue;
        }
        let ind = indent_of(l);
        if i > start && ind <= parent {
            return None;
        }
        let body = if i == start && parent >= 0 {
            &l[(parent as usize).min(l.len())..]
        } else {
            l
        };
        for n in &needles {
            if let Some(pos) = find_token(body, n) {
                let offset = l.len() - body.len();
                return Some((i, (offset + pos) as isize));
            }
        }
    }
    None
}

/// `n`th `- ` item below the parent.
fn find_item(lines: &[&str], start: usize, parent: isize, n: usize) -> Option<(usize, isize)> {
    let mut seen = 0;
    let mut item_indent = None;
    for (i, l) in lines.iter().enumerate().skip(start + 1) {
        if is_blank(l) {
            continue;
        }
        let ind = indent_of(l);
        if ind <= parent && !l.trim_start().starts_with("- ") {
            return None;
        }
        if l.trim_start().starts_with("- ") || l.trim() == "-" {
            match item_indent {
                None => item_indent = Some(ind),
                Some(ii) if ii != ind => continue,
                _ => {}
            }
            if seen == n {
                return Some((i, ind + 1));
            }
            seen += 1;
        }
    }
    None
}

/// Position of `needle` as a key token (start of text, or after `{`, `,`,
/// `- `, whitespace).
fn find_token(hay: &str, needle: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(p) = hay[from..].find(needle) {
        let at = from + p;
        let before = hay[..at].trim_end();
        if before.is_empty()
            || before.ends_with('{')
            || before.ends_with(',')
            || before.ends_with('-')
        {
            return Some(at);
        }
        from = at + needle.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const Y: &str = "id: dev\nstates:\n  A:\n    on:\n      go: B\n      stay:\n        - guard: {type: visits}\n          target: A\n        - target: B\n  B: { type: final, entry: { type: prompt } }\n";

    #[test]
    fn finds_block_and_flow_keys_and_items() {
        assert_eq!(locate(Y, &["id"]), Some(1));
        assert_eq!(locate(Y, &["states", "A", "on", "go"]), Some(5));
        assert_eq!(locate(Y, &["states", "A", "on", "stay", "[1]"]), Some(9));
        assert_eq!(
            locate(Y, &["states", "A", "on", "stay", "[0]", "guard"]),
            Some(7)
        );
        assert_eq!(locate(Y, &["states", "B", "entry"]), Some(10));
        // Missing → deepest ancestor.
        assert_eq!(locate(Y, &["states", "A", "exit"]), Some(3));
        assert_eq!(locate(Y, &["nope"]), None);
        assert_eq!(locate(Y, &["states", "A", "on", "stay", "[7]"]), Some(6));
    }
}
