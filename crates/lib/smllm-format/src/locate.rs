//! Find the line of a YAML path in block- or flow-style YAML, for findings
//! about well-formed files (serde reports its own locations).
//!
//! Keys match only as direct children of their parent: in block style at the
//! parent's child indentation, in a flow map `{…}` at its top nesting level.
//! So `states` never matches `- states: [...]` inside `meta.sharedActions`,
//! and a state's `description` never matches a transition's.

/// Where a located node is written.
#[derive(Debug, Clone, Copy)]
struct Node {
    line: usize,
    /// Column of the key, or of the `-` of a list item.
    col: usize,
    /// Column where the node's value (or item content) starts on `line`.
    value_col: usize,
    /// A block list item: its content may hold the first key on this line.
    item: bool,
}

/// The 1-based line where `path` (keys; `[n]` list items) is written, or the
/// deepest ancestor found.
pub fn locate(text: &str, path: &[&str]) -> Option<usize> {
    let lines: Vec<&str> = text.lines().collect();
    let mut cur: Option<Node> = None;
    let mut found = None;
    for seg in path {
        let next = match seg.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            Some(idx) => idx.parse().ok().and_then(|n| item(&lines, cur, n)),
            None => key(&lines, cur, seg),
        };
        match next {
            Some(n) => {
                found = Some(n.line + 1);
                cur = Some(n);
            }
            None => break,
        }
    }
    found
}

fn indent_of(l: &str) -> usize {
    l.len() - l.trim_start().len()
}

fn is_blank(l: &str) -> bool {
    let t = l.trim();
    t.is_empty() || t.starts_with('#')
}

/// The key forms `key:`, `"key":`, `'key':`, followed by a space or the end.
fn key_len_at(text: &str, key: &str) -> Option<usize> {
    for needle in [
        format!("{key}:"),
        format!("\"{key}\":"),
        format!("'{key}':"),
    ] {
        if let Some(rest) = text.strip_prefix(needle.as_str())
            && (rest.is_empty() || rest.starts_with([' ', '\t']))
        {
            return Some(needle.len());
        }
    }
    None
}

/// A key directly under `parent` (`None` = the document root).
fn key(lines: &[&str], parent: Option<Node>, key: &str) -> Option<Node> {
    let Some(p) = parent else {
        return block_key(lines, 0, None, key);
    };
    let line = lines.get(p.line)?;
    let rest = line.get(p.value_col..).unwrap_or("");
    if p.item {
        // `- key: v` — the item's first key is on its own line, at value_col.
        if let Some(len) = key_len_at(rest, key) {
            return Some(Node {
                line: p.line,
                col: p.value_col,
                value_col: p.value_col + len,
                item: false,
            });
        }
        let t = rest.trim_start();
        if t.starts_with('{') {
            return flow_key(line, p.value_col + (rest.len() - t.len()), p.line, key);
        }
        return block_key_at(lines, p.line + 1, p.value_col, key);
    }
    let t = rest.trim_start();
    if t.starts_with('{') {
        return flow_key(line, p.value_col + (rest.len() - t.len()), p.line, key);
    }
    // A one-item list is written without its `[0]` in finding paths
    // (OneOrMany): look inside the first item.
    if t.is_empty()
        && let Some(first) = (p.line + 1..lines.len()).find(|&i| !is_blank(lines[i]))
        && lines[first].trim_start().starts_with('-')
    {
        let first_item = item(lines, Some(p), 0)?;
        return self::key(lines, Some(first_item), key);
    }
    block_key(lines, p.line + 1, Some(p.col), key)
}

/// A block key below a parent at `parent_col`: the first content line sets
/// the child indentation; only keys there match.
fn block_key(lines: &[&str], start: usize, parent_col: Option<usize>, key: &str) -> Option<Node> {
    let first = (start..lines.len()).find(|&i| !is_blank(lines[i]))?;
    let col = indent_of(lines[first]);
    if parent_col.is_some_and(|pc| col <= pc) || lines[first].trim_start().starts_with('-') {
        return None;
    }
    block_key_at(lines, first, col, key)
}

/// A key at exactly `col`, from `start` until the block ends (a line
/// indented less than `col`).
fn block_key_at(lines: &[&str], start: usize, col: usize, key: &str) -> Option<Node> {
    for (i, l) in lines.iter().enumerate().skip(start) {
        if is_blank(l) {
            continue;
        }
        let ind = indent_of(l);
        if ind < col {
            return None;
        }
        if ind == col
            && let Some(len) = key_len_at(&l[ind..], key)
        {
            return Some(Node {
                line: i,
                col: ind,
                value_col: ind + len,
                item: false,
            });
        }
    }
    None
}

/// A key at the top level of the flow map opening at `open` on `line`.
fn flow_key(line: &str, open: usize, line_no: usize, key: &str) -> Option<Node> {
    let bytes = line.as_bytes();
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut expect_key = true;
    let mut i = open + 1;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            b',' if depth == 0 => {
                expect_key = true;
                i += 1;
                continue;
            }
            b' ' | b'\t' => {
                i += 1;
                continue;
            }
            _ => {}
        }
        if depth == 0 && expect_key {
            if let Some(len) = key_len_at(&line[i..], key) {
                return Some(Node {
                    line: line_no,
                    col: i,
                    value_col: i + len,
                    item: false,
                });
            }
            expect_key = false;
        }
        if c == b'"' || c == b'\'' {
            quote = Some(c);
        }
        i += 1;
    }
    None
}

/// The `n`th item of the list under `parent`.
fn item(lines: &[&str], parent: Option<Node>, n: usize) -> Option<Node> {
    let p = parent?;
    let line = lines.get(p.line)?;
    if line
        .get(p.value_col..)
        .unwrap_or("")
        .trim_start()
        .starts_with('[')
    {
        // A flow list: its items share the line.
        return Some(Node {
            line: p.line,
            col: p.value_col,
            value_col: p.value_col,
            item: false,
        });
    }
    let mut item_col = None;
    let mut seen = 0;
    for (i, l) in lines.iter().enumerate().skip(p.line + 1) {
        if is_blank(l) {
            continue;
        }
        let ind = indent_of(l);
        let t = l.trim_start();
        let dash = t == "-" || t.starts_with("- ");
        match item_col {
            None if dash && ind >= p.col => item_col = Some(ind),
            None => return None,
            Some(c) if ind < c || (ind == c && !dash) => return None,
            _ => {}
        }
        if dash && Some(ind) == item_col {
            if seen == n {
                let content = ind + 1 + t[1..].len() - t[1..].trim_start().len();
                return Some(Node {
                    line: i,
                    col: ind,
                    value_col: content,
                    item: true,
                });
            }
            seen += 1;
        }
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
        assert_eq!(
            locate(Y, &["states", "A", "on", "stay", "[0]", "target"]),
            Some(8)
        );
        assert_eq!(locate(Y, &["states", "B", "entry"]), Some(10));
        assert_eq!(locate(Y, &["states", "B", "entry", "type"]), Some(10));
        // Missing → deepest ancestor.
        assert_eq!(locate(Y, &["states", "A", "exit"]), Some(3));
        assert_eq!(locate(Y, &["nope"]), None);
        assert_eq!(locate(Y, &["states", "A", "on", "stay", "[7]"]), Some(6));
    }

    #[test]
    fn keys_match_only_as_direct_children() {
        // sharedActions items carry a `states:` key before the real one.
        let y = "id: x\nmeta:\n  sharedActions:\n    - states: [A]\n      position: before\nstates:\n  A:\n    on:\n      go: { description: t, target: A }\n    description: d\n";
        assert_eq!(locate(y, &["states"]), Some(6));
        assert_eq!(locate(y, &["states", "A", "on", "go"]), Some(9));
        assert_eq!(locate(y, &["states", "A", "description"]), Some(10));
        assert_eq!(locate(y, &["states", "A", "on", "go", "target"]), Some(9));
        assert_eq!(
            locate(y, &["meta", "sharedActions", "[0]", "position"]),
            Some(5)
        );
        // A nested flow map's keys are not the outer map's.
        let f = "a: { b: { c: 1 }, c: 2 }\n";
        assert_eq!(locate(f, &["a", "c"]), Some(1));
        assert_eq!(locate(f, &["a", "b", "c"]), Some(1));
        assert_eq!(locate("a:\n- x\n", &["a", "[0]"]), Some(2));
        // A one-item list without its index: the key inside the item.
        let l = "on:\n  go:\n    - guard: g\n      target: B\n";
        assert_eq!(locate(l, &["on", "go", "target"]), Some(4));
    }
}
