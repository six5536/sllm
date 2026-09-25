// Derived from sokf 9c93f37 crates/lib/sokf-core/tests/properties_harness.rs
//! The correctness properties of the harness integration: a region touches
//! only its block, a merge keeps the rest, the states partition, install is
//! idempotent, a refusal writes nothing, a block is compared by words, and
//! the loop guard blocks once per text.

mod common;

use agent_harness_kit::{
    InstallOptions, LoopGuard, MergeOp, Scope,
    harness::{self, Found, Markers, Observed, State},
    install, status,
};
use common::{PREFIX, TempTree, hooks_ops, mcp_ops, permissions_ops};
use proptest::prelude::*;

const P: &str = ".claude/settings.json";

fn m() -> Markers {
    Markers::new("tool")
}

fn arb_key() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "model",
        "env",
        "theme",
        "cleanupPeriodDays",
        "statusLine",
    ])
    .prop_map(str::to_string)
}

fn arb_scalar() -> impl Strategy<Value = serde_json::Value> {
    prop_oneof![
        any::<bool>().prop_map(serde_json::Value::from),
        (0i64..1000).prop_map(serde_json::Value::from),
        "[a-z]{1,8}".prop_map(serde_json::Value::from),
    ]
}

/// A settings object: other keys before and after the tool's, an `allow`
/// list with or without the entry, hook lists with or without the tool's
/// groups and with other groups, an `mcpServers` object, and a nested object
/// whose order must survive.
fn arb_settings_object() -> impl Strategy<Value = serde_json::Value> {
    (
        prop::collection::vec((arb_key(), arb_scalar()), 0..3),
        prop::option::of(prop::collection::vec(
            prop::sample::select(vec!["Bash(npm run *)", "mcp__tool", "Read(./src/**)"]),
            0..3,
        )),
        prop::option::of(prop::collection::vec(
            prop::sample::select(vec!["echo hi", "tool harness hook claude stop", "make test"]),
            0..3,
        )),
        prop::option::of(prop::sample::select(vec!["other", "tool"])),
        prop::collection::vec((arb_key(), arb_scalar()), 0..3),
    )
        .prop_map(|(before, allow, stop, server, after)| {
            let mut map = serde_json::Map::new();
            for (k, v) in before {
                map.insert(k, v);
            }
            if let Some(allow) = allow {
                map.insert(
                    "permissions".into(),
                    serde_json::json!({ "allow": allow, "deny": ["Bash(rm *)"] }),
                );
            }
            if let Some(stop) = stop {
                let groups: Vec<_> = stop
                    .into_iter()
                    .map(|c| serde_json::json!({ "hooks": [{ "type": "command", "command": c }] }))
                    .collect();
                map.insert(
                    "hooks".into(),
                    serde_json::json!({ "PreToolUse": [], "Stop": groups.clone(), "SessionStart": groups }),
                );
            }
            if let Some(name) = server {
                map.insert("mcpServers".into(), serde_json::json!({ name: { "command": "x" }, "z": {} }));
            }
            for (k, v) in after {
                map.insert(k, v);
            }
            map.insert("nested".into(), serde_json::json!({ "z": 1, "a": { "y": 2, "b": 3 } }));
            serde_json::Value::Object(map)
        })
}

fn arb_indent() -> impl Strategy<Value = String> {
    prop::sample::select(vec!["  ", "    ", "\t"]).prop_map(str::to_string)
}

/// The value with the tool's entries of `ops` removed, nothing else touched.
fn strip_entries(v: &serde_json::Value, ops: &[MergeOp]) -> serde_json::Value {
    let mut v = v.clone();
    for op in ops {
        harness::remove_merge(&mut v, op);
    }
    v
}

fn keys_in_order(v: &serde_json::Value) -> Vec<String> {
    v.as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

fn arb_text() -> impl Strategy<Value = String> {
    prop::collection::vec("[a-z #@-]{0,12}", 0..6).prop_map(|lines| lines.join("\n"))
}

fn arb_block() -> impl Strategy<Value = String> {
    prop::collection::vec("[a-zA-Z.][a-zA-Z .]{0,19}", 1..4)
        .prop_map(|lines| format!("{}\n", lines.join("\n")))
}

fn arb_ops() -> impl Strategy<Value = Vec<MergeOp>> {
    prop::sample::select(vec![0usize, 1, 2]).prop_map(|i| match i {
        0 => hooks_ops(),
        1 => permissions_ops(),
        _ => mcp_ops(),
    })
}

/// The markers' lines removed with the block between them.
fn strip_block(t: &str) -> String {
    let lines: Vec<&str> = t.split('\n').collect();
    match (
        lines.iter().position(|l| *l == "<!-- tool:harness -->"),
        lines.iter().position(|l| *l == "<!-- /tool:harness -->"),
    ) {
        (Some(o), Some(c)) if c > o => [&lines[..o], &lines[c + 1..]].concat().join("\n"),
        _ => t.to_string(),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn region_touches_only_its_block(text in arb_text(), block in arb_block(), crlf in any::<bool>()) {
        let text = if crlf { text.replace('\n', "\r\n") } else { text };
        let Some(after) = harness::render_region(Some(&text), &block, &m()) else {
            let found = harness::find_region(&text, &m());
            prop_assert_eq!(found.as_deref(), Some(block.as_str()));
            return Ok(());
        };
        let lf = after.replace("\r\n", "\n");
        prop_assert_eq!(lf.matches("<!-- tool:harness -->").count(), 1);
        prop_assert!(lf.contains("<!-- tool:harness -->\n\n"), "a blank line after the opening marker");
        let found = harness::find_region(&after, &m());
        prop_assert_eq!(found.as_deref(), Some(block.as_str()));
        let kept = strip_block(&lf);
        let original = strip_block(&text.replace("\r\n", "\n"));
        prop_assert!(kept.starts_with(original.trim_end_matches('\n')), "{kept:?} vs {original:?}");
        prop_assert_eq!(harness::render_region(Some(&after), &block, &m()), None);
        prop_assert_eq!(after.contains("\r\n"), text.contains("\r\n"));
    }

    #[test]
    fn merge_keeps_the_rest(object in arb_settings_object(), indent in arb_indent(), newline in any::<bool>(), ops in arb_ops()) {
        let text = harness::json_text(&object, &indent, newline);
        let Some(after) = harness::render_merge(Some(&text), &ops, P).unwrap() else {
            for op in &ops {
                prop_assert_eq!(harness::extract(&object, op), Some(op.value().clone()));
            }
            return Ok(());
        };
        let parsed: serde_json::Value = serde_json::from_str(&after).unwrap();
        for op in &ops {
            prop_assert_eq!(harness::extract(&parsed, op), Some(op.value().clone()));
        }
        // Everything but the tool's entries survives; containers the input
        // lacked are created and hold nothing else.
        let container = match &ops[0] {
            MergeOp::HookGroup { .. } => "hooks",
            MergeOp::ArrayEntry { .. } => "permissions",
            MergeOp::ObjectMember { .. } => "mcpServers",
        };
        let mut stripped = strip_entries(&parsed, &ops);
        if object.get(container).is_none() {
            stripped.as_object_mut().unwrap().shift_remove(container);
        } else if container == "hooks" {
            // Events the input lacked are created, empty once stripped.
            let hooks = stripped["hooks"].as_object_mut().unwrap();
            hooks.retain(|k, v| object["hooks"].get(k).is_some() || v != &serde_json::json!([]));
        }
        prop_assert_eq!(stripped, strip_entries(&object, &ops));
        let mut expected = keys_in_order(&object);
        if !expected.iter().any(|k| k == container) {
            expected.push(container.to_string());
        }
        prop_assert_eq!(keys_in_order(&parsed), expected);
        prop_assert_eq!(harness::indent_of(&after), indent);
        prop_assert_eq!(after.ends_with('\n'), newline);
        prop_assert_eq!(harness::render_merge(Some(&after), &ops, P).unwrap(), None);
    }

    #[test]
    fn states_partition(found in prop::option::of(arb_block()), recorded_of_found in any::<bool>(), declined in any::<bool>()) {
        let expected = Found::Block("embedded\n".into());
        let observed = match &found {
            Some(b) => Observed::Present(Found::Block(b.clone())),
            None => Observed::Absent,
        };
        let recorded = match &found {
            Some(b) if recorded_of_found => Some(harness::hash(&Found::Block(b.clone()))),
            _ => Some("fnv1a64:0000000000000000".to_string()),
        };
        let state = harness::state(&observed, &expected, recorded.as_deref(), declined);
        let want = if declined {
            State::Skipped
        } else if found.is_none() {
            State::Absent
        } else if found.as_deref().unwrap().split_whitespace().eq(["embedded"]) {
            State::Current
        } else if recorded_of_found {
            State::Stale
        } else {
            State::Edited
        };
        prop_assert_eq!(state, want);
        let hash = harness::hash(&expected);
        prop_assert_eq!(harness::state(&Observed::Present(expected.clone()), &expected, Some(&hash), false), State::Current);
    }

    #[test]
    fn a_block_is_compared_by_words(block in arb_block(), seps in prop::collection::vec(prop::sample::select(vec![" ", "  ", "\n", "\r\n", "\t", " \n\n"]), 1..40)) {
        let words: Vec<&str> = block.split_whitespace().collect();
        let mut reflowed = String::new();
        for (i, w) in words.iter().enumerate() {
            reflowed.push_str(w);
            reflowed.push_str(seps[i % seps.len()]);
        }
        let expected = Found::Block(block.clone());
        let found = Found::Block(reflowed);
        prop_assert_eq!(harness::state(&Observed::Present(found.clone()), &expected, None, false), State::Current);
        prop_assert_eq!(harness::hash(&found), harness::hash(&expected));
    }

    #[test]
    fn the_guard_blocks_once_per_text(texts in prop::collection::vec(prop::sample::select(vec!["", "1 error", "2 errors"]), 1..10)) {
        let tree = TempTree::empty("guard");
        let guard = LoopGuard::new(tree.dir().join("cache"));
        let mut last: Option<&str> = None;
        for text in texts {
            let blocked = guard.should_block("session", text);
            prop_assert_eq!(blocked, last != Some(text));
            if blocked {
                last = Some(text);
            }
        }
    }
}

/// A tree with an instructions file of either kind, a settings file of any
/// style, and a declined list; `install` twice.
#[derive(Debug, Clone)]
struct Scenario {
    agents: Option<String>,
    claude: Option<String>,
    settings: Option<(serde_json::Value, String, bool)>,
    without: Vec<&'static str>,
}

fn arb_scenario() -> impl Strategy<Value = Scenario> {
    (
        prop::option::of(arb_text()),
        prop::option::of(prop_oneof![Just("@AGENTS.md\n".to_string()), arb_text()]),
        prop::option::of((arb_settings_object(), arb_indent(), any::<bool>())),
        prop::collection::btree_set(
            prop::sample::select(vec![
                "skills",
                "instructions",
                "mcp",
                "hooks",
                "permissions",
            ]),
            0..3,
        ),
    )
        .prop_map(|(agents, claude, settings, without)| Scenario {
            agents,
            claude,
            settings,
            without: without.into_iter().collect(),
        })
}

fn build(s: &Scenario) -> TempTree {
    let tree = TempTree::empty("scenario");
    if let Some(t) = &s.agents {
        tree.write("AGENTS.md", t);
    }
    if let Some(t) = &s.claude {
        tree.write("CLAUDE.md", t);
    }
    if let Some((object, indent, newline)) = &s.settings {
        tree.write(
            ".claude/settings.json",
            &harness::json_text(object, indent, *newline),
        );
    }
    tree
}

fn opts(without: Vec<String>, name: &str) -> InstallOptions {
    InstallOptions {
        harness: name.into(),
        scope: Scope::Project,
        without: (!without.is_empty()).then_some(without),
        force: false,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn install_is_idempotent(s in arb_scenario()) {
        let tree = build(&s);
        let o = opts(s.without.iter().map(|w| w.to_string()).collect(), "claude");
        // An existing tool group or member the tool never wrote is edited;
        // force takes it over so the second run can be checked.
        install(&tree.tool(), &InstallOptions { force: true, ..o.clone() }).unwrap();
        let after_first = tree.files();
        let second = install(&tree.tool(), &o).unwrap();
        for p in &second.parts {
            prop_assert!(p.state == "current" || p.state == "skipped", "{second:?}");
            prop_assert_eq!(p.state == "skipped", s.without.contains(&p.part.as_str()));
        }
        prop_assert_eq!(tree.files(), after_first);
        let st = status(&tree.tool(), "claude", Scope::Project).unwrap();
        let expected: Vec<_> = second.parts.iter().map(|p| p.state.clone()).collect();
        let got: Vec<_> = st.parts.iter().map(|p| p.state.clone()).collect();
        prop_assert_eq!(got, expected);
    }

    #[test]
    fn nothing_on_refusal(s in arb_scenario(), which in 0u8..5) {
        let tree = build(&s);
        let mut without: Vec<String> = s.without.iter().map(|w| w.to_string()).collect();
        let mut name = "claude";
        match which {
            0 => name = "cursor",
            1 => without.push("nope".into()),
            2 => tree.write(".claude/settings.json", "{ not json"),
            3 => {
                // A container of another type refuses when the part is written.
                without.retain(|w| w != "mcp");
                tree.write(".mcp.json", "{\"mcpServers\": 1}");
            }
            _ => tree.write(".tool/harness.toml", "[claude\nx = "),
        }
        let before = tree.files();
        let mut o = opts(without, name);
        o.without.get_or_insert_with(Vec::new);
        let e = install(&tree.tool(), &o).unwrap_err();
        prop_assert!(matches!(e, agent_harness_kit::Error::Harness(_)), "{e}");
        prop_assert_eq!(tree.files(), before);
    }
}

#[test]
fn prefix_is_the_test_tools() {
    assert!(hooks_ops().iter().all(
        |op| matches!(op, MergeOp::HookGroup { command_prefix, .. } if command_prefix == PREFIX)
    ));
}
