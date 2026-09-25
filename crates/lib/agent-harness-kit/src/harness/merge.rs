// Derived from sokf 9c93f37 crates/lib/sokf-core/src/harness/merge.rs
//! The `merge` kind: the tool owns entries in the user's JSON file, named by
//! data-driven operations. Key order, the file's indent and its trailing
//! newline are kept.

use serde::Serialize;
use serde_json::{Map, Value, ser::PrettyFormatter};

use crate::{Error, Result};

/// One entry the tool owns in a JSON file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOp {
    /// `value` is present once in the array at `path`, e.g.
    /// `["permissions", "allow"]`. Found by equality.
    ArrayEntry {
        /// The keys from the top-level object to the array.
        path: Vec<String>,
        /// The entry.
        value: Value,
    },
    /// The member `key` of the object at `path` is `value`, e.g. `smllm`
    /// under `["mcpServers"]`. Found by its key.
    ObjectMember {
        /// The keys from the top-level object to the object; empty for the
        /// top-level object itself.
        path: Vec<String>,
        /// The member's key.
        key: String,
        /// The member's value.
        value: Value,
    },
    /// The tool's group under `hooks.<event>` is `group`: it replaces the
    /// group one of whose `hooks[].command` starts with `command_prefix`, or
    /// is appended.
    HookGroup {
        /// The hook event, e.g. `Stop` or `SessionStart`.
        event: String,
        /// The prefix that identifies the tool's command, e.g.
        /// `smllm harness hook `.
        command_prefix: String,
        /// The group, e.g. `{"hooks": [{"type": "command", "command": …}]}`.
        group: Value,
    },
}

impl MergeOp {
    /// An [`MergeOp::ArrayEntry`] at a dotted path such as
    /// `permissions.allow`.
    pub fn array_entry(path: &str, value: impl Into<Value>) -> Self {
        MergeOp::ArrayEntry {
            path: split(path),
            value: value.into(),
        }
    }

    /// An [`MergeOp::ObjectMember`] of the object at a dotted path such as
    /// `mcpServers` (`""` for the top level).
    pub fn object_member(path: &str, key: impl Into<String>, value: Value) -> Self {
        MergeOp::ObjectMember {
            path: split(path),
            key: key.into(),
            value,
        }
    }

    /// A [`MergeOp::HookGroup`] with a single command hook: `{"hooks":
    /// [{"type": "command", "command": command}]}`.
    pub fn hook_command(
        event: impl Into<String>,
        command_prefix: impl Into<String>,
        command: &str,
    ) -> Self {
        MergeOp::HookGroup {
            event: event.into(),
            command_prefix: command_prefix.into(),
            group: serde_json::json!({ "hooks": [{ "type": "command", "command": command }] }),
        }
    }

    /// The value the operation writes.
    pub fn value(&self) -> &Value {
        match self {
            MergeOp::ArrayEntry { value, .. } | MergeOp::ObjectMember { value, .. } => value,
            MergeOp::HookGroup { group, .. } => group,
        }
    }
}

fn split(path: &str) -> Vec<String> {
    path.split('.')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Parse a target file. A file that does not parse, or is not an object, is
/// a refusal naming `display`.
pub fn parse_json(display: &str, text: &str) -> Result<Value> {
    let doc: Value = serde_json::from_str(text)
        .map_err(|e| Error::Harness(format!("{display}: does not parse as JSON: {e}")))?;
    if !doc.is_object() {
        return Err(Error::Harness(format!("{display}: is not a JSON object")));
    }
    Ok(doc)
}

/// The indent of a file: the leading whitespace of its first indented line,
/// or two spaces.
pub fn indent_of(text: &str) -> String {
    text.lines()
        .find(|l| l.starts_with(' ') || l.starts_with('\t'))
        .map(|l| l[..l.len() - l.trim_start().len()].to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "  ".to_string())
}

/// The value at `path` in `doc`, when every step is an object member.
fn get<'a>(doc: &'a Value, path: &[String]) -> Option<&'a Value> {
    path.iter().try_fold(doc, |v, k| v.as_object()?.get(k))
}

/// Whether a hook group is the tool's: one of its commands starts with the
/// prefix.
fn is_tool_group(group: &Value, prefix: &str) -> bool {
    group["hooks"].as_array().is_some_and(|hooks| {
        hooks
            .iter()
            .any(|h| h["command"].as_str().is_some_and(|c| c.starts_with(prefix)))
    })
}

/// The tool's entry of `op` as found in `doc`; `None` when it is not there.
pub fn extract(doc: &Value, op: &MergeOp) -> Option<Value> {
    match op {
        MergeOp::ArrayEntry { path, value } => get(doc, path)?
            .as_array()?
            .iter()
            .find(|v| *v == value)
            .cloned(),
        MergeOp::ObjectMember { path, key, .. } => get(doc, path)?.as_object()?.get(key).cloned(),
        MergeOp::HookGroup {
            event,
            command_prefix,
            ..
        } => doc["hooks"][event.as_str()]
            .as_array()?
            .iter()
            .find(|g| is_tool_group(g, command_prefix))
            .cloned(),
    }
}

/// The object at `path` in `doc`, created along the way when absent.
fn object_at<'a>(
    doc: &'a mut Value,
    path: &[String],
    display: &str,
) -> Result<&'a mut Map<String, Value>> {
    let mut cur = doc;
    for (i, key) in path.iter().enumerate() {
        let map = cur
            .as_object_mut()
            .ok_or_else(|| not_object(display, &path[..i]))?;
        cur = map
            .entry(key.clone())
            .or_insert_with(|| Value::Object(Map::new()));
    }
    cur.as_object_mut().ok_or_else(|| not_object(display, path))
}

fn not_object(display: &str, path: &[String]) -> Error {
    Error::Harness(format!("{display}: `{}` is not an object", path.join(".")))
}

/// The array at `path` (non-empty) in `doc`, created along the way when
/// absent.
fn array_at<'a>(doc: &'a mut Value, path: &[String], display: &str) -> Result<&'a mut Vec<Value>> {
    let (last, parents) = path
        .split_last()
        .ok_or_else(|| Error::Internal("an array entry needs a path".into()))?;
    object_at(doc, parents, display)?
        .entry(last.clone())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| Error::Harness(format!("{display}: `{}` is not an array", path.join("."))))
}

/// Set the tool's entry of `op` in `doc`. Containers are created when
/// absent; a container of another type is a refusal naming `display`.
pub fn apply(doc: &mut Value, op: &MergeOp, display: &str) -> Result<()> {
    match op {
        MergeOp::ArrayEntry { path, value } => {
            let array = array_at(doc, path, display)?;
            if !array.contains(value) {
                array.push(value.clone());
            }
        }
        MergeOp::ObjectMember { path, key, value } => {
            object_at(doc, path, display)?.insert(key.clone(), value.clone());
        }
        MergeOp::HookGroup {
            event,
            command_prefix,
            group,
        } => {
            let path = ["hooks".to_string(), event.clone()];
            let groups = array_at(doc, &path, display)?;
            match groups.iter_mut().find(|g| is_tool_group(g, command_prefix)) {
                Some(existing) => *existing = group.clone(),
                None => groups.push(group.clone()),
            }
        }
    }
    Ok(())
}

/// Remove the tool's entry of `op` from `doc`, leaving the containers.
/// Returns whether anything was removed.
pub fn remove(doc: &mut Value, op: &MergeOp) -> bool {
    fn get_mut<'a>(doc: &'a mut Value, path: &[String]) -> Option<&'a mut Value> {
        path.iter()
            .try_fold(doc, |v, k| v.as_object_mut()?.get_mut(k))
    }
    match op {
        MergeOp::ArrayEntry { path, value } => {
            let Some(array) = get_mut(doc, path).and_then(Value::as_array_mut) else {
                return false;
            };
            let before = array.len();
            array.retain(|v| v != value);
            array.len() != before
        }
        MergeOp::ObjectMember { path, key, .. } => get_mut(doc, path)
            .and_then(Value::as_object_mut)
            .is_some_and(|m| m.shift_remove(key).is_some()),
        MergeOp::HookGroup {
            event,
            command_prefix,
            ..
        } => {
            let path = ["hooks".to_string(), event.clone()];
            let Some(groups) = get_mut(doc, &path).and_then(Value::as_array_mut) else {
                return false;
            };
            let before = groups.len();
            groups.retain(|g| !is_tool_group(g, command_prefix));
            groups.len() != before
        }
    }
}

/// Serialise with an indent and, when asked, a trailing newline.
pub fn json_text(doc: &Value, indent: &str, trailing_newline: bool) -> String {
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(
        &mut buf,
        PrettyFormatter::with_indent(indent.as_bytes()),
    );
    doc.serialize(&mut ser).expect("a JSON value serialises");
    let mut text = String::from_utf8(buf).expect("serde_json writes UTF-8");
    if trailing_newline {
        text.push('\n');
    }
    text
}

/// Re-serialise `doc` in the style of `existing`: its indent (two spaces
/// for a new file) and its trailing newline (one for a new file). `None`
/// when the text is unchanged.
fn restyle(doc: &Value, existing: Option<&str>) -> Option<String> {
    let indent = existing.map_or_else(|| "  ".to_string(), indent_of);
    let newline = existing.is_none_or(|t| t.ends_with('\n'));
    let out = json_text(doc, &indent, newline);
    (Some(out.as_str()) != existing).then_some(out)
}

/// The file to write with `ops` merged into `existing`; `None` when nothing
/// changes. `display` names the file in a refusal.
pub fn render_merge(
    existing: Option<&str>,
    ops: &[MergeOp],
    display: &str,
) -> Result<Option<String>> {
    let mut doc = match existing {
        Some(text) => parse_json(display, text)?,
        None => Value::Object(Map::new()),
    };
    for op in ops {
        apply(&mut doc, op, display)?;
    }
    Ok(restyle(&doc, existing))
}

/// The file to write with the entries of `ops` removed from `existing`;
/// `None` when nothing changes or the file is absent.
pub fn render_unmerge(
    existing: Option<&str>,
    ops: &[MergeOp],
    display: &str,
) -> Result<Option<String>> {
    let Some(text) = existing else {
        return Ok(None);
    };
    let mut doc = parse_json(display, text)?;
    let mut changed = false;
    for op in ops {
        changed |= remove(&mut doc, op);
    }
    Ok(if changed {
        restyle(&doc, existing)
    } else {
        None
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const P: &str = ".claude/settings.json";
    const PREFIX: &str = "tool harness hook ";

    fn permissions() -> Vec<MergeOp> {
        vec![MergeOp::array_entry("permissions.allow", "Bash(tool *)")]
    }

    fn hooks() -> Vec<MergeOp> {
        vec![MergeOp::hook_command(
            "Stop",
            PREFIX,
            "tool harness hook claude stop",
        )]
    }

    #[test]
    fn indent_is_detected_or_two_spaces() {
        assert_eq!(indent_of("{\n    \"a\": 1\n}"), "    ");
        assert_eq!(indent_of("{\n\t\"a\": 1\n}"), "\t");
        assert_eq!(indent_of("{}"), "  ");
        assert_eq!(indent_of(""), "  ");
    }

    #[test]
    fn permissions_are_added_once_and_extracted() {
        let out = render_merge(None, &permissions(), P).unwrap().unwrap();
        assert_eq!(
            out,
            "{\n  \"permissions\": {\n    \"allow\": [\n      \"Bash(tool *)\"\n    ]\n  }\n}\n"
        );
        let doc = parse_json(P, &out).unwrap();
        assert_eq!(
            extract(&doc, &permissions()[0]),
            Some(json!("Bash(tool *)"))
        );
        let existing = "{\n  \"permissions\": {\n    \"allow\": [\n      \"Bash(npm *)\",\n      \"Bash(tool *)\"\n    ]\n  }\n}\n";
        assert_eq!(
            render_merge(Some(existing), &permissions(), P).unwrap(),
            None
        );
        let doc = parse_json(P, "{\"permissions\":{\"allow\":[\"Bash(npm *)\"]}}").unwrap();
        assert_eq!(extract(&doc, &permissions()[0]), None);
        assert_eq!(extract(&json!({"permissions": 1}), &permissions()[0]), None);
    }

    #[test]
    fn the_hook_group_replaces_the_tools_group_and_keeps_others() {
        let existing = r#"{
    "other": true,
    "hooks": {
        "PreToolUse": [],
        "Stop": [
            { "hooks": [ { "type": "command", "command": "echo hi" } ] },
            { "hooks": [ { "type": "command", "command": "tool harness hook claude old" } ] }
        ]
    },
    "z": [1, 2]
}"#;
        let out = render_merge(Some(existing), &hooks(), P).unwrap().unwrap();
        let doc = parse_json(P, &out).unwrap();
        let keys: Vec<_> = doc.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["other", "hooks", "z"]);
        assert_eq!(doc["hooks"]["PreToolUse"], json!([]));
        assert_eq!(doc["hooks"]["Stop"][0]["hooks"][0]["command"], "echo hi");
        assert_eq!(
            doc["hooks"]["Stop"][1]["hooks"][0]["command"],
            "tool harness hook claude stop"
        );
        assert_eq!(doc["hooks"]["Stop"].as_array().unwrap().len(), 2);
        assert!(out.starts_with("{\n    \"other\": true,"), "{out}");
        assert!(!out.ends_with('\n'));
        assert_eq!(extract(&doc, &hooks()[0]), Some(hooks()[0].value().clone()));
        assert_eq!(render_merge(Some(&out), &hooks(), P).unwrap(), None);
        let out = render_merge(Some("{}\n"), &hooks(), P).unwrap().unwrap();
        let doc = parse_json(P, &out).unwrap();
        assert_eq!(doc["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn hook_groups_for_several_events_and_an_object_member() {
        let ops = vec![
            MergeOp::hook_command(
                "SessionStart",
                PREFIX,
                "tool harness hook claude session-start",
            ),
            MergeOp::hook_command("Stop", PREFIX, "tool harness hook claude stop"),
            MergeOp::object_member(
                "mcpServers",
                "tool",
                json!({"command": "tool", "args": ["mcp"]}),
            ),
            MergeOp::object_member("", "top", json!(1)),
        ];
        let out = render_merge(Some("{\"mcpServers\": {\"x\": {}}}"), &ops, P)
            .unwrap()
            .unwrap();
        let doc = parse_json(P, &out).unwrap();
        let keys: Vec<_> = doc["mcpServers"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(keys, ["x", "tool"]);
        assert_eq!(doc["top"], 1);
        for op in &ops {
            assert_eq!(extract(&doc, op).as_ref(), Some(op.value()));
        }
        // Each event holds its own group.
        assert_eq!(doc["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
        assert_eq!(doc["hooks"]["Stop"].as_array().unwrap().len(), 1);
        // A member of another value is found, so it can be compared.
        let edited = json!({"mcpServers": {"tool": {"command": "other"}}});
        assert_eq!(extract(&edited, &ops[2]), Some(json!({"command": "other"})));
        assert_eq!(render_merge(Some(&out), &ops, P).unwrap(), None);
    }

    #[test]
    fn unmerge_removes_only_the_tools_entries() {
        let ops = vec![
            MergeOp::array_entry("permissions.allow", "Bash(tool *)"),
            MergeOp::hook_command("Stop", PREFIX, "tool harness hook claude stop"),
            MergeOp::object_member("mcpServers", "tool", json!({})),
        ];
        let text = "{\n  \"permissions\": {\"allow\": [\"a\", \"Bash(tool *)\"]},\n  \"hooks\": {\"Stop\": [{\"hooks\": [{\"command\": \"echo\"}]}, {\"hooks\": [{\"command\": \"tool harness hook claude stop\"}]}]},\n  \"mcpServers\": {\"tool\": {}, \"y\": 1}\n}\n";
        let out = render_unmerge(Some(text), &ops, P).unwrap().unwrap();
        let doc = parse_json(P, &out).unwrap();
        assert_eq!(doc["permissions"]["allow"], json!(["a"]));
        assert_eq!(doc["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert_eq!(doc["mcpServers"], json!({"y": 1}));
        assert_eq!(render_unmerge(Some(&out), &ops, P).unwrap(), None);
        assert_eq!(render_unmerge(Some("{}"), &ops, P).unwrap(), None);
        assert_eq!(render_unmerge(None, &ops, P).unwrap(), None);
        assert!(render_unmerge(Some("["), &ops, P).is_err());
    }

    #[test]
    fn a_file_that_does_not_parse_is_refused_and_named() {
        let e = render_merge(Some("{ nope"), &hooks(), P).unwrap_err();
        assert!(matches!(e, Error::Harness(_)), "{e}");
        assert!(
            e.to_string()
                .starts_with(".claude/settings.json: does not parse"),
            "{e}"
        );
        let e = parse_json(P, "[]").unwrap_err();
        assert!(e.to_string().contains("not a JSON object"), "{e}");
        let e = render_merge(Some("{\"permissions\": []}"), &permissions(), P).unwrap_err();
        assert!(
            e.to_string().contains("`permissions` is not an object"),
            "{e}"
        );
        let e = render_merge(
            Some("{\"permissions\": {\"allow\": {}}}"),
            &permissions(),
            P,
        )
        .unwrap_err();
        assert!(
            e.to_string()
                .contains("`permissions.allow` is not an array"),
            "{e}"
        );
        let e = render_merge(Some("{\"hooks\": []}"), &hooks(), P).unwrap_err();
        assert!(e.to_string().contains("`hooks` is not an object"), "{e}");
        let e = render_merge(
            Some("{\"m\": 1}"),
            &[MergeOp::object_member("m", "k", json!(1))],
            P,
        )
        .unwrap_err();
        assert!(e.to_string().contains("`m` is not an object"), "{e}");
        let bad = MergeOp::ArrayEntry {
            path: vec![],
            value: json!(1),
        };
        assert!(matches!(
            render_merge(None, &[bad], P),
            Err(Error::Internal(_))
        ));
    }
}
