// Derived from sokf 9c93f37 crates/lib/sokf-core/src/harness/state.rs
//! The state of a part and the hash the record keeps of its content.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::{
    Error, Result,
    harness::{Content, Markers, Part, extract, find_region, parse_json},
    hash::{Fnv, normalise},
};

/// The state of a part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// Declined by the user.
    Skipped,
    /// The file, the block or the entries are not there.
    Absent,
    /// Equal to the tool's content.
    Current,
    /// Differs from the tool's content and equals the recorded hash: the
    /// tool wrote it and has changed since.
    Stale,
    /// Differs from both, or present with no recorded hash.
    Edited,
}

impl State {
    /// The state's name in a report.
    pub fn as_str(self) -> &'static str {
        match self {
            State::Skipped => "skipped",
            State::Absent => "absent",
            State::Current => "current",
            State::Stale => "stale",
            State::Edited => "edited",
        }
    }
}

/// A part's content in a comparable form: what the tool wants, or what the
/// tree holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// A `file` part's files: (relative path, text).
    Files(Vec<(String, String)>),
    /// A `region` part's block, compared by words.
    Block(String),
    /// A `merge` part's entries that are present, in operation order.
    Entries(Vec<Value>),
    /// An `external` part's text.
    Text(String),
}

/// What the tree holds for a part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observed {
    /// Nothing the tool could own.
    Absent,
    /// The part's content as found.
    Present(Found),
}

/// The content the tool wants for `part`.
pub fn expected(part: &Part) -> Found {
    match &part.content {
        Content::Files(files) => Found::Files(files.clone()),
        Content::Block(block) => Found::Block(block.clone()),
        Content::Merge(ops) => Found::Entries(ops.iter().map(|op| op.value().clone()).collect()),
        Content::External(ext) => Found::Text(ext.expected()),
    }
}

/// The text of `path`; `None` when absent.
pub(crate) fn read_text(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(path, e)),
    }
}

/// Read a part's content at `path` (relative to `root`), through its kind.
/// A `merge` file that does not parse is a refusal.
pub fn observe(root: &Path, part: &Part, path: &str, markers: &Markers) -> Result<Observed> {
    let fs_path = root.join(path);
    Ok(match &part.content {
        Content::Files(files) => {
            let mut found = Vec::new();
            for (rel, _) in files {
                let Some(text) = read_text(&fs_path.join(rel))? else {
                    return Ok(Observed::Absent);
                };
                found.push((rel.clone(), text));
            }
            Observed::Present(Found::Files(found))
        }
        Content::Block(_) => match read_text(&fs_path)?.and_then(|t| find_region(&t, markers)) {
            Some(block) => Observed::Present(Found::Block(block)),
            None => Observed::Absent,
        },
        Content::Merge(ops) => match read_text(&fs_path)? {
            Some(text) => {
                let doc = parse_json(path, &text)?;
                let entries: Vec<Value> = ops.iter().filter_map(|op| extract(&doc, op)).collect();
                if entries.is_empty() {
                    Observed::Absent
                } else {
                    Observed::Present(Found::Entries(entries))
                }
            }
            None => Observed::Absent,
        },
        Content::External(ext) => match ext.observe()? {
            Some(text) => Observed::Present(Found::Text(text)),
            None => Observed::Absent,
        },
    })
}

/// The words of a text: whitespace-insensitive comparison.
fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split_whitespace()
}

/// Whether two contents are equal: files and text line endings aside, a
/// block by its words.
fn equal(a: &Found, b: &Found) -> bool {
    match (a, b) {
        (Found::Files(x), Found::Files(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y)
                    .all(|((p, s), (q, t))| p == q && normalise(s) == normalise(t))
        }
        (Found::Block(x), Found::Block(y)) => words(x).eq(words(y)),
        (Found::Entries(x), Found::Entries(y)) => x == y,
        (Found::Text(x), Found::Text(y)) => normalise(x) == normalise(y),
        _ => false,
    }
}

/// The state of a part from what the tree holds, the tool's content and the
/// recorded hash.
pub fn state(
    observed: &Observed,
    expected: &Found,
    recorded: Option<&str>,
    declined: bool,
) -> State {
    if declined {
        return State::Skipped;
    }
    match observed {
        Observed::Absent => State::Absent,
        Observed::Present(found) => {
            if equal(found, expected) {
                State::Current
            } else if recorded.is_some_and(|h| h == hash(found)) {
                State::Stale
            } else {
                State::Edited
            }
        }
    }
}

/// The FNV-1a hash of a content, as `fnv1a64:` and sixteen hex digits:
/// files with LF line endings and their paths; a block's words joined by
/// single spaces; entries as compact JSON; text with LF line endings.
pub fn hash(content: &Found) -> String {
    let mut h = Fnv::new();
    match content {
        Found::Files(files) => {
            for (path, text) in files {
                h.update(path.as_bytes());
                h.update(&[0]);
                h.update(normalise(text).as_bytes());
                h.update(&[0]);
            }
        }
        Found::Block(text) => h.update(words(text).collect::<Vec<_>>().join(" ").as_bytes()),
        Found::Entries(v) => h.update(Value::Array(v.clone()).to_string().as_bytes()),
        Found::Text(text) => h.update(normalise(text).as_bytes()),
    }
    h.render()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::hash_text;

    fn block(s: &str) -> Found {
        Found::Block(s.into())
    }

    #[test]
    fn the_hash_is_fnv1a() {
        assert_eq!(hash(&block("")), "fnv1a64:cbf29ce484222325");
        assert_eq!(hash(&block("a")), "fnv1a64:af63dc4c8601ec8c");
        // A block hashes its words.
        assert_eq!(hash(&block("x\r\n  y\n")), hash(&block("x y")));
        // Files hash their paths too, so a rename changes the hash.
        let a = Found::Files(vec![("A.md".into(), "t\n".into())]);
        let b = Found::Files(vec![("B.md".into(), "t\r\n".into())]);
        assert_ne!(hash(&a), hash(&b));
        let e = Found::Entries(vec![serde_json::json!({"a": [1, 2]})]);
        assert_eq!(hash(&e), hash_text("[{\"a\":[1,2]}]"));
        assert_eq!(hash(&Found::Text("t\r\n".into())), hash_text("t\n"));
    }

    #[test]
    fn a_block_is_compared_by_words() {
        let expected = block("Use the tool.\nAlways.\n");
        let reflowed = Observed::Present(block("Use  the\ntool.   Always.\r\n\n"));
        assert_eq!(state(&reflowed, &expected, None, false), State::Current);
        let other = Observed::Present(block("Use a tool. Always."));
        assert_eq!(state(&other, &expected, None, false), State::Edited);
    }

    #[test]
    fn each_row_of_the_state_table() {
        let expected = block("new\n");
        let old = block("old\n");
        let recorded = hash(&old);
        assert_eq!(
            state(&Observed::Present(expected.clone()), &expected, None, true),
            State::Skipped
        );
        assert_eq!(
            state(&Observed::Absent, &expected, None, false),
            State::Absent
        );
        assert_eq!(
            state(&Observed::Present(block("new\r\n")), &expected, None, false),
            State::Current
        );
        assert_eq!(
            state(
                &Observed::Present(old.clone()),
                &expected,
                Some(&recorded),
                false
            ),
            State::Stale
        );
        assert_eq!(
            state(&Observed::Present(old.clone()), &expected, None, false),
            State::Edited
        );
        assert_eq!(
            state(
                &Observed::Present(block("other\n")),
                &expected,
                Some(&recorded),
                false
            ),
            State::Edited
        );
        assert_eq!(
            state(
                &Observed::Present(Found::Files(vec![])),
                &expected,
                None,
                false
            ),
            State::Edited
        );
        let files = Found::Files(vec![("a".into(), "x\n".into())]);
        assert_eq!(
            state(
                &Observed::Present(Found::Files(vec![("a".into(), "x\r\n".into())])),
                &files,
                None,
                false
            ),
            State::Current
        );
        assert_eq!(
            state(
                &Observed::Present(Found::Text("a\r\n".into())),
                &Found::Text("a\n".into()),
                None,
                false
            ),
            State::Current
        );
        assert_eq!(State::Stale.as_str(), "stale");
        assert_eq!(serde_json::to_string(&State::Edited).unwrap(), "\"edited\"");
    }
}
