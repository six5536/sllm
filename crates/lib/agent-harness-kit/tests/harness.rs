// Derived from sokf 9c93f37 crates/lib/sokf-core/tests/harness.rs
//! `install` and `status` on temporary trees: each scenario through the
//! test tool, the tree, the record and the result checked, and every
//! refusal leaving the tree byte-identical.

mod common;

use agent_harness_kit::{Error, HarnessResult, InstallOptions, Result, Scope, install, status};
use common::{INSTRUCTIONS, SKILL, TempTree};

fn run(
    tree: &TempTree,
    scope: Scope,
    without: Option<&[&str]>,
    force: bool,
) -> Result<HarnessResult> {
    install(
        &tree.tool(),
        &InstallOptions {
            harness: "claude".into(),
            scope,
            without: without.map(|w| w.iter().map(|s| s.to_string()).collect()),
            force,
        },
    )
}

fn project(tree: &TempTree, without: Option<&[&str]>, force: bool) -> HarnessResult {
    run(tree, Scope::Project, without, force).unwrap()
}

fn states(result: &HarnessResult) -> Vec<&str> {
    result.parts.iter().map(|p| p.state.as_str()).collect()
}

fn st(tree: &TempTree) -> Vec<String> {
    status(&tree.tool(), "claude", Scope::Project)
        .unwrap()
        .parts
        .into_iter()
        .map(|p| p.state)
        .collect()
}

#[test]
fn an_empty_project_gets_every_part_and_the_record() {
    let tree = TempTree::empty("empty");
    let out = project(&tree, None, false);
    let rows: Vec<_> = out
        .parts
        .iter()
        .map(|p| (p.part.as_str(), p.state.as_str(), p.path.as_str()))
        .collect();
    assert_eq!(
        rows,
        [
            ("skills", "created", ".claude/skills/tool"),
            ("instructions", "created", "CLAUDE.md"),
            ("mcp", "created", ".mcp.json"),
            ("hooks", "created", ".claude/settings.json"),
            ("permissions", "created", ".claude/settings.json"),
        ]
    );
    assert_eq!(out.scope, Scope::Project);
    assert_eq!(out.root, tree.dir().display().to_string());
    assert_eq!(tree.read(".claude/skills/tool/SKILL.md"), SKILL);
    assert_eq!(
        tree.read("CLAUDE.md"),
        format!("<!-- tool:harness -->\n\n{INSTRUCTIONS}<!-- /tool:harness -->\n")
    );
    let settings: serde_json::Value =
        serde_json::from_str(&tree.read(".claude/settings.json")).unwrap();
    assert_eq!(settings["permissions"]["allow"][0], "mcp__tool");
    let keys: Vec<_> = settings["hooks"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(keys, ["SessionStart", "UserPromptSubmit", "Stop"]);
    assert_eq!(
        settings["hooks"]["Stop"][0]["hooks"][0]["command"],
        "tool harness hook claude stop"
    );
    assert_eq!(
        tree.read(".mcp.json"),
        "{\n  \"mcpServers\": {\n    \"tool\": {\n      \"command\": \"tool\",\n      \"args\": [\n        \"mcp\"\n      ]\n    }\n  }\n}\n"
    );
    let record = tree.read(".tool/harness.toml");
    assert!(
        record.starts_with("# Written by tool harness install. Do not edit.\n\n[claude]\n"),
        "{record}"
    );
    for part in ["hooks", "instructions", "mcp", "permissions", "skills"] {
        assert!(record.contains(&format!("{part} = \"fnv1a64:")), "{record}");
    }
    assert!(!tree.exists(".tool/config.toml"));
    // Idempotent: the second run writes nothing and reports current.
    let before = tree.files();
    let again = project(&tree, None, false);
    assert!(
        again.parts.iter().all(|p| p.state == "current"),
        "{again:?}"
    );
    assert_eq!(tree.files(), before);
    assert!(st(&tree).iter().all(|s| s == "current"));
}

#[test]
fn the_instructions_target_follows_the_tree() {
    let tree = TempTree::empty("agents");
    tree.write("AGENTS.md", "# Agents\n");
    let out = project(&tree, None, false);
    assert_eq!(out.parts[1].path, "AGENTS.md");
    assert_eq!(out.parts[1].state, "updated");
    assert!(
        tree.read("AGENTS.md")
            .starts_with("# Agents\n\n<!-- tool:harness -->\n\n")
    );
    assert!(!tree.exists("CLAUDE.md"));
    let tree = TempTree::empty("import");
    tree.write("AGENTS.md", "# Agents\n");
    tree.write("CLAUDE.md", "@AGENTS.md\n");
    let out = project(&tree, None, false);
    assert_eq!(out.parts[1].path, "AGENTS.md");
    assert_eq!(tree.read("CLAUDE.md"), "@AGENTS.md\n");
    let tree = TempTree::empty("both");
    tree.write("AGENTS.md", "# Agents\n");
    tree.write("CLAUDE.md", "# Claude\n");
    let out = project(&tree, None, false);
    assert_eq!(out.parts[1].path, "CLAUDE.md");
    assert_eq!(tree.read("AGENTS.md"), "# Agents\n");
    assert!(
        tree.read("CLAUDE.md")
            .starts_with("# Claude\n\n<!-- tool:harness -->\n")
    );
}

#[test]
fn a_reflowed_block_is_current_and_left_alone() {
    let tree = TempTree::empty("reflow");
    project(&tree, None, false);
    let reflowed = tree
        .read("CLAUDE.md")
        .replace("uses tool.\nRead", "uses   tool.  Read")
        .replace('\n', "\r\n");
    tree.write("CLAUDE.md", &reflowed);
    let before = tree.files();
    let out = project(&tree, None, false);
    assert_eq!(out.parts[1].state, "current");
    assert_eq!(tree.files(), before);
}

#[test]
fn a_settings_file_of_another_style_keeps_its_entries_order_and_indent() {
    let tree = TempTree::empty("style");
    tree.write(
        ".claude/settings.json",
        "{\n    \"permissions\": {\n        \"allow\": [\"Bash(npm run *)\"]\n    },\n    \"hooks\": {\n        \"Stop\": [{\"hooks\": [{\"type\": \"command\", \"command\": \"echo done\"}]}]\n    },\n    \"model\": \"opus\"\n}",
    );
    let out = project(&tree, None, false);
    assert_eq!(out.parts[3].state, "updated");
    assert_eq!(out.parts[4].state, "updated");
    let after = tree.read(".claude/settings.json");
    let doc: serde_json::Value = serde_json::from_str(&after).unwrap();
    let keys: Vec<_> = doc.as_object().unwrap().keys().cloned().collect();
    assert_eq!(keys, ["permissions", "hooks", "model"]);
    assert_eq!(
        doc["permissions"]["allow"],
        serde_json::json!(["Bash(npm run *)", "mcp__tool"])
    );
    assert_eq!(doc["hooks"]["Stop"][0]["hooks"][0]["command"], "echo done");
    assert_eq!(
        doc["hooks"]["Stop"][1]["hooks"][0]["command"],
        "tool harness hook claude stop"
    );
    let keys: Vec<_> = doc["hooks"].as_object().unwrap().keys().cloned().collect();
    assert_eq!(keys, ["Stop", "SessionStart", "UserPromptSubmit"]);
    assert!(
        after.starts_with("{\n    \"permissions\": {\n        \"allow\": ["),
        "{after}"
    );
    assert!(!after.ends_with('\n'), "no trailing newline was there");
    assert!(st(&tree).iter().all(|s| s == "current"));
}

#[test]
fn a_part_the_tool_never_wrote_is_edited_until_forced() {
    let tree = TempTree::empty("foreign");
    tree.write(".claude/skills/tool/SKILL.md", "mine\n");
    tree.write(
        ".mcp.json",
        "{\"mcpServers\": {\"tool\": {\"command\": \"old\"}}}\n",
    );
    let out = project(&tree, None, false);
    assert_eq!(out.parts[0].state, "edited");
    assert_eq!(out.parts[2].state, "edited");
    assert_eq!(tree.read(".claude/skills/tool/SKILL.md"), "mine\n");
    let record = tree.read(".tool/harness.toml");
    assert!(
        !record.contains("skills") && !record.contains("mcp"),
        "{record}"
    );
    assert_eq!(st(&tree)[0], "edited");
    let out = project(&tree, None, true);
    assert_eq!(out.parts[0].state, "rewrote");
    assert_eq!(out.parts[2].state, "updated");
    assert_eq!(tree.read(".claude/skills/tool/SKILL.md"), SKILL);
    assert!(
        tree.read(".tool/harness.toml")
            .contains("skills = \"fnv1a64:")
    );
}

#[test]
fn every_part_edited_then_install_then_force() {
    let tree = TempTree::empty("edited");
    project(&tree, None, false);
    tree.write(".claude/skills/tool/SKILL.md", "changed\n");
    tree.write(
        "CLAUDE.md",
        &tree.read("CLAUDE.md").replace("Read the", "Skip the"),
    );
    let settings = tree
        .read(".claude/settings.json")
        .replace(
            "tool harness hook claude stop",
            "tool harness hook claude stop --quiet",
        )
        .replace("mcp__tool", "mcp__other");
    tree.write(".claude/settings.json", &settings);
    tree.write(
        ".mcp.json",
        &tree.read(".mcp.json").replace("\"mcp\"", "\"serve\""),
    );
    // The permission entry is gone, so absent; the hook group is still the
    // tool's by its prefix, so edited.
    assert_eq!(
        st(&tree),
        ["edited", "edited", "edited", "edited", "absent"]
    );
    let out = project(&tree, None, false);
    assert_eq!(
        states(&out),
        ["edited", "edited", "edited", "edited", "updated"]
    );
    assert_eq!(tree.read(".claude/skills/tool/SKILL.md"), "changed\n");
    assert!(tree.read("CLAUDE.md").contains("Skip the"));
    assert!(tree.read(".claude/settings.json").contains("--quiet"));
    let out = project(&tree, None, true);
    assert_eq!(
        states(&out),
        ["rewrote", "updated", "updated", "updated", "current"]
    );
    assert!(st(&tree).iter().all(|s| s == "current"));
}

#[test]
fn a_changed_profile_makes_written_parts_stale() {
    let tree = TempTree::empty("stale");
    project(&tree, None, false);
    // An older version wrote only the Stop group and another block: the
    // record holds the hashes of what it wrote.
    let settings = tree.read(".claude/settings.json");
    let mut doc: serde_json::Value = serde_json::from_str(&settings).unwrap();
    doc["hooks"]
        .as_object_mut()
        .unwrap()
        .shift_remove("SessionStart");
    doc["hooks"]
        .as_object_mut()
        .unwrap()
        .shift_remove("UserPromptSubmit");
    tree.write(
        ".claude/settings.json",
        &serde_json::to_string_pretty(&doc).unwrap(),
    );
    tree.write(
        "CLAUDE.md",
        "<!-- tool:harness -->\nOld text.\n<!-- /tool:harness -->\n",
    );
    let stop = agent_harness_kit::harness::Found::Entries(vec![doc["hooks"]["Stop"][0].clone()]);
    let old = agent_harness_kit::harness::Found::Block("Old  text.".into());
    let record = tree.read(".tool/harness.toml");
    let record = record
        .lines()
        .map(|l| {
            if l.starts_with("hooks = ") {
                format!("hooks = \"{}\"", agent_harness_kit::harness::hash(&stop))
            } else if l.starts_with("instructions = ") {
                format!(
                    "instructions = \"{}\"",
                    agent_harness_kit::harness::hash(&old)
                )
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    tree.write(".tool/harness.toml", &format!("{record}\n"));
    assert_eq!(
        st(&tree),
        ["current", "stale", "current", "stale", "current"]
    );
    let out = project(&tree, None, false);
    assert_eq!(
        states(&out),
        ["current", "updated", "current", "updated", "current"]
    );
    assert!(st(&tree).iter().all(|s| s == "current"));
}

#[test]
fn a_declined_part_stays_declined_until_the_config_line_goes() {
    let tree = TempTree::empty("declined");
    let out = project(&tree, Some(&["hooks"]), false);
    assert_eq!(out.parts[3].state, "skipped");
    assert_eq!(
        tree.read(".tool/config.toml"),
        "[harness.claude]\nwithout = [\"hooks\"]\n"
    );
    assert!(!tree.read(".claude/settings.json").contains("hooks"));
    let out = project(&tree, None, false);
    assert_eq!(out.parts[3].state, "skipped");
    assert_eq!(st(&tree)[3], "skipped");
    let out = project(&tree, Some(&["skills"]), false);
    assert_eq!(
        states(&out),
        ["skipped", "current", "current", "updated", "current"]
    );
    assert_eq!(
        tree.read(".tool/config.toml"),
        "[harness.claude]\nwithout = [\"skills\"]\n"
    );
    assert!(!tree.read(".tool/harness.toml").contains("skills = "));
    tree.write(".tool/config.toml", "# nothing declined\n");
    let out = project(&tree, None, false);
    assert!(out.parts.iter().all(|p| p.state == "current"), "{out:?}");
    assert!(tree.read(".tool/harness.toml").contains("skills = "));
}

#[test]
fn the_user_scope_has_its_own_root_record_and_external_part() {
    let tree = TempTree::empty("user");
    let out = run(&tree, Scope::User, Some(&["skills"]), false).unwrap();
    let rows: Vec<_> = out
        .parts
        .iter()
        .map(|p| (p.part.as_str(), p.state.as_str(), p.path.as_str()))
        .collect();
    assert_eq!(
        rows,
        [
            ("skills", "skipped", ".claude/skills/tool"),
            ("instructions", "created", "CLAUDE.md"),
            ("mcp", "created", "claude mcp (user)"),
            ("hooks", "created", ".claude/settings.json"),
            ("permissions", "created", ".claude/settings.json"),
        ]
    );
    assert!(tree.exists("home/.claude/CLAUDE.md"));
    assert!(tree.exists("home/claude-mcp-user.json"));
    assert!(tree.exists("home/.config/tool/harness.toml"));
    assert_eq!(
        tree.read("home/.config/tool/config.toml"),
        "[harness.claude]\nwithout = [\"skills\"]\n"
    );
    assert!(!tree.exists("CLAUDE.md") && !tree.exists(".tool"));
    let again = run(&tree, Scope::User, None, false).unwrap();
    assert_eq!(
        states(&again),
        ["skipped", "current", "current", "current", "current"]
    );
    // The external part edited: left, then forced.
    tree.write("home/claude-mcp-user.json", "{}");
    let st = status(&tree.tool(), "claude", Scope::User).unwrap();
    assert_eq!(st.parts[2].state, "edited");
    assert_eq!(
        run(&tree, Scope::User, None, false).unwrap().parts[2].state,
        "edited"
    );
    assert_eq!(
        run(&tree, Scope::User, None, true).unwrap().parts[2].state,
        "updated"
    );
    assert_eq!(
        status(&tree.tool(), "claude", Scope::User).unwrap().parts[2].state,
        "current"
    );
}

#[test]
fn json_and_text_carry_the_same_parts() {
    let tree = TempTree::empty("json");
    let out = project(&tree, Some(&["permissions"]), false);
    let json = serde_json::to_value(&out).unwrap();
    assert_eq!(json["harness"], "claude");
    assert_eq!(json["scope"], "project");
    assert_eq!(json["parts"].as_array().unwrap().len(), 5);
    assert_eq!(
        json["parts"][4],
        serde_json::json!({ "part": "permissions", "state": "skipped", "path": ".claude/settings.json" })
    );
    let keys: Vec<_> = json["parts"][0]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(keys, ["part", "state", "path"]);
    let keys: Vec<_> = json.as_object().unwrap().keys().cloned().collect();
    assert_eq!(keys, ["harness", "scope", "root", "parts"]);
    insta::assert_snapshot!(out.to_text(), @r"
    created .claude/skills/tool (skills)
    created CLAUDE.md (instructions)
    created .mcp.json (mcp)
    created .claude/settings.json (hooks)
    skipped .claude/settings.json (permissions)
    ");
}

#[test]
fn every_refusal_leaves_the_tree_byte_identical() {
    let tree = TempTree::empty("refusals");
    tree.write("CLAUDE.md", "# Mine\n");
    let pristine = tree.files();
    let refused = |tree: &TempTree, name: &str, without: Option<&[&str]>| -> String {
        let e = install(
            &tree.tool(),
            &InstallOptions {
                harness: name.into(),
                scope: Scope::Project,
                without: without.map(|w| w.iter().map(|s| s.to_string()).collect()),
                force: false,
            },
        )
        .unwrap_err();
        assert!(matches!(e, Error::Harness(_)), "{e}");
        e.to_string()
    };
    assert_eq!(refused(&tree, "cursor", None), "no profile named `cursor`");
    assert_eq!(tree.files(), pristine);
    assert_eq!(
        refused(&tree, "claude", Some(&["hooks", "mcp2"])),
        "profile `claude` has no part named `mcp2`"
    );
    assert_eq!(tree.files(), pristine);
    for (file, text, message) in [
        (
            ".claude/settings.json",
            "{ \"permissions\": [ }",
            ".claude/settings.json: does not parse",
        ),
        (".mcp.json", "[]", ".mcp.json: is not a JSON object"),
        (
            ".claude/settings.json",
            "{\"hooks\": []}",
            "`hooks` is not an object",
        ),
        (".tool/harness.toml", "[claude\n", ".tool/harness.toml: "),
        (
            ".tool/config.toml",
            "[harness.claude]\nwithout = 1\n",
            ".tool/config.toml: ",
        ),
    ] {
        tree.write(file, text);
        let before = tree.files();
        let e = refused(&tree, "claude", None);
        assert!(e.contains(message), "{e}");
        assert_eq!(tree.files(), before, "{e}");
        std::fs::remove_file(tree.dir().join(file)).unwrap();
    }
    tree.write(".tool/config.toml", "harness = 1\n");
    assert!(status(&tree.tool(), "claude", Scope::Project).is_err());
    assert!(status(&tree.tool(), "cursor", Scope::Project).is_err());
    std::fs::remove_file(tree.dir().join(".tool/config.toml")).unwrap();
    std::fs::remove_dir(tree.dir().join(".tool")).unwrap();
    std::fs::remove_dir(tree.dir().join(".claude")).unwrap();
    assert_eq!(tree.files(), pristine);
}
