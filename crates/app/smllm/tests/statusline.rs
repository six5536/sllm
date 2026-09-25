//! `smllm statusline` end to end: Claude Code's status JSON on stdin, the
//! row or JSON out, and never a non-zero exit (STL).

mod common;

use common::{World, key_in};
use serde_json::{Value, json};

fn status_json(w: &World, sid: &str) -> String {
    json!({ "session_id": sid, "cwd": w.project, "model": { "display_name": "Opus" } }).to_string()
}

// @zen-test: STL-1_AC-1
// @zen-test: STL-2_AC-1
// @zen-test: CLI-14_AC-1
#[test]
fn the_row_follows_the_bound_session() {
    let w = World::showcase("statusline");
    let v = w.hook(
        "session-start",
        json!({ "session_id": "cc-1", "cwd": w.project, "hook_event_name": "SessionStart", "source": "startup" }),
    );
    let key = key_in(
        v["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap(),
    );
    let stdin = status_json(&w, "cc-1");

    let o = w.run_stdin(&["statusline", "--color", "never"], &stdin);
    assert_eq!(
        (o.code, o.stdout.as_str()),
        (0, "smllm idle"),
        "{}",
        o.stderr
    );

    let o = w.run(&[
        "fire",
        "--session",
        &key,
        "enter",
        "--param",
        "stateMachine=showcase",
    ]);
    assert_eq!(o.code, 0, "{}", o.stdout);
    let o = w.run_stdin(&["statusline", "--color", "never"], &stdin);
    assert!(
        o.stdout.starts_with("smllm showcase › DRAFT · document i-"),
        "{}",
        o.stdout
    );
    // Colour by default, off with NO_COLOR.
    let o = w.run_stdin(&["statusline"], &stdin);
    assert!(o.stdout.contains("\x1b[1mDRAFT\x1b[0m"), "{}", o.stdout);
    let o = w
        .cmd()
        .args(["statusline"])
        .env("NO_COLOR", "1")
        .write_stdin(stdin.clone())
        .output()
        .unwrap();
    assert!(!String::from_utf8(o.stdout).unwrap().contains('\x1b'));

    let o = w.run_stdin(&["statusline", "--json"], &stdin);
    let s: Value = serde_json::from_str(&o.stdout).unwrap();
    assert_eq!(s["session"], key.as_str());
    assert_eq!(s["state"], "DRAFT");
    assert_eq!(s["instance"]["kind"], "document");
    // --session wins, and needs no stdin.
    let o = w.run(&["statusline", "--session", &key, "--json"]);
    assert_eq!(serde_json::from_str::<Value>(&o.stdout).unwrap(), s);
}

// @zen-test: STL-2_AC-2
// @zen-test: STL-4_AC-1
#[test]
fn nothing_to_show_prints_nothing_and_exits_0() {
    let w = World::showcase("statusline-empty");
    for (args, stdin) in [
        (vec!["statusline"], status_json(&w, "never-bound")),
        (vec!["statusline"], "not json".to_string()),
        (vec!["statusline"], String::new()),
        (vec!["statusline", "--session", "sm-nope"], String::new()),
    ] {
        let o = w.run_stdin(&args, &stdin);
        assert_eq!((o.code, o.stdout.as_str()), (0, ""), "{args:?} {stdin}");
        let mut json_args = args.clone();
        json_args.push("--json");
        let o = w.run_stdin(&json_args, &stdin);
        assert_eq!((o.code, o.stdout.as_str()), (0, "{}\n"), "{args:?} {stdin}");
    }
}

// @zen-test: STL-9_AC-2
// @zen-test: STL-9_AC-3
// @zen-test: STL-11_AC-1
#[test]
fn install_adds_the_skill_and_hints_until_the_status_line_calls_smllm() {
    let w = World::showcase("statusline-skill");
    let o = w.run(&["harness", "install", "claude"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(
        o.stdout
            .contains("created .claude/skills/smllm-statusline (statusline)"),
        "{}",
        o.stdout
    );
    let plugin =
        std::fs::read_to_string(common::repo().join("plugin/skills/smllm-statusline/SKILL.md"))
            .unwrap();
    assert_eq!(
        w.read(".claude/skills/smllm-statusline/SKILL.md"),
        plugin,
        "the plugin ships the same skill"
    );
    assert!(o.stderr.contains("ask Claude to add it"), "{}", o.stderr);
    // Install never sets statusLine.
    assert!(!w.read(".claude/settings.json").contains("statusLine"));

    w.write(
        ".claude/settings.local.json",
        r#"{"statusLine":{"type":"command","command":"smllm statusline"}}"#,
    );
    let o = w.run(&["harness", "status", "claude"]);
    assert!(!o.stderr.contains("ask Claude"), "{}", o.stderr);

    // Declined: no skill, no hint.
    let w = World::showcase("statusline-without");
    let o = w.run(&["harness", "install", "claude", "--without", "statusline"]);
    assert!(!w.project.join(".claude/skills").exists());
    assert!(!o.stderr.contains("ask Claude"), "{}", o.stderr);
    let o = w.run(&["harness", "status", "claude"]);
    assert!(o.stdout.contains("skipped"), "{}", o.stdout);
    assert!(!o.stderr.contains("ask Claude"), "{}", o.stderr);
}
