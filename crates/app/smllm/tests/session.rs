//! Scripted sessions (TEST-2): a fake agent drives the Claude Code hooks,
//! `smllm fire` and the MCP server through the showcase machine.

mod common;

use std::io::{BufRead as _, BufReader, Write as _};
use std::process::{Command, Stdio};

use common::{World, key_in};
use serde_json::{Value, json};

fn start(w: &World, sid: &str) -> String {
    let v = w.hook("session-start", json!({ "session_id": sid, "cwd": w.project, "hook_event_name": "SessionStart", "source": "startup" }));
    let ctx = v["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(v["hookSpecificOutput"]["hookEventName"], "SessionStart");
    assert!(ctx.contains("· idle"), "{ctx}");
    key_in(&ctx)
}

fn fire(w: &World, key: &str, event: &str, params: &[&str]) -> (i32, String) {
    let mut args = vec!["fire", "--session", key, event];
    for p in params {
        args.extend(["--param", p]);
    }
    let o = w.run(&args);
    assert!(o.code == 0 || o.code == 1, "{}", o.stderr);
    (o.code, o.stdout)
}

fn stop(w: &World, sid: &str, active: bool) -> Value {
    w.hook(
        "stop",
        json!({ "session_id": sid, "hook_event_name": "Stop", "stop_hook_active": active }),
    )
}

// @zen-test: TURN-4_AC-1
// @zen-test: TURN-5_AC-1
// @zen-test: TURN-6_AC-1
// @zen-test: TURN-8_AC-1
// @zen-test: HOST-7_AC-1
// @zen-test: HOST-4_AC-1
// @zen-test: CLI-4_AC-1
// @zen-test: CLI-5_AC-1
// @zen-test: CLI-6_AC-1
// @zen-test: CLI-8_AC-1
#[test]
fn hooks_and_fire_through_the_showcase() {
    let w = World::showcase("session");
    let key = start(&w, "cc-1");
    // Same harness session id → same key, now as a resume.
    assert_eq!(start(&w, "cc-1"), key);
    assert_eq!(stop(&w, "cc-1", false), json!({}), "idle may stop");

    let (code, out) = fire(&w, &key, "enter", &["stateMachine=showcase"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("showcase › DRAFT · document i-"), "{out}");
    assert!(
        out.contains("Work on one document at a time.\n\nWrite the draft."),
        "{out}"
    );

    // Stop is blocked with the events list until yield.
    let v = stop(&w, "cc-1", false);
    assert_eq!(v["decision"], "block");
    let menu = v["reason"].as_str().unwrap();
    assert!(
        menu.contains("- submit — Select when every section of the draft is written."),
        "{menu}"
    );
    assert!(
        menu.contains("summary (required): One line for the reviewer"),
        "{menu}"
    );
    let o = w.run_stdin(
        &["harness", "hook", "claude", "stop"],
        &json!({ "session_id": "cc-1", "stop_hook_active": true }).to_string(),
    );
    assert_eq!(o.stdout.trim(), "{}", "runaway guard allows the stop");
    assert!(o.stderr.contains("<events>"), "list goes to stderr");
    let (code, _) = fire(&w, &key, "yield", &["note=asking"]);
    assert_eq!(code, 0);
    assert_eq!(stop(&w, "cc-1", false), json!({}));
    assert_eq!(
        w.hook(
            "user-prompt-submit",
            json!({ "session_id": "cc-1", "prompt": "go on" })
        ),
        json!({})
    );
    assert_eq!(stop(&w, "cc-1", false)["decision"], "block");

    // setRef with a pattern; the check guard reads the file named by the ref.
    let (code, out) = fire(&w, &key, "named", &["documentPath=nope"]);
    assert_eq!(code, 1);
    assert!(out.contains("must match"), "{out}");
    let (code, _) = fire(&w, &key, "named", &["documentPath=docs/intro.md"]);
    assert_eq!(code, 0);
    let (_, out) = fire(&w, &key, "submit", &["summary=first"]);
    assert!(
        out.contains("DRAFT (visit 2)") && out.contains("via CHECK"),
        "{out}"
    );
    assert!(
        out.contains("The document file is missing or empty."),
        "{out}"
    );
    w.write("docs/intro.md", "Intro.\n");
    let (_, out) = fire(&w, &key, "submit", &["summary=second"]);
    assert!(out.contains("› REVIEW"), "{out}");
    assert!(out.contains("Checklist: goal stated"), "{out}");

    // Detour through the fallback state and back.
    let (_, out) = fire(&w, &key, "unmatched", &[]);
    assert!(out.contains("› DETOUR"), "{out}");
    let (_, out) = fire(&w, &key, "resume", &[]);
    assert!(out.contains("› REVIEW (visit 2)"), "{out}");

    // Park, and a second session takes the instance over.
    let (_, out) = fire(&w, &key, "park", &[]);
    assert!(
        out.contains("Parked:\n- document docs/intro.md (showcase) at REVIEW"),
        "{out}"
    );
    let key2 = start(&w, "cc-2");
    assert_ne!(key2, key);
    fire(
        &w,
        &key,
        "enter",
        &["stateMachine=showcase", "documentPath=docs/intro.md"],
    );
    let (_, out) = fire(
        &w,
        &key2,
        "enter",
        &["stateMachine=showcase", "documentPath=docs/intro.md"],
    );
    assert!(
        out.contains(&format!("Took over from session {key}")),
        "{out}"
    );
    let (code, out) = fire(&w, &key, "approve", &[]);
    assert_eq!(code, 1);
    assert!(out.contains(&format!("moved to session {key2}")), "{out}");

    // Approve runs a list-form command (git add in a non-repo fails → reported,
    // never blocking), then the final state completes the instance.
    let (code, out) = fire(&w, &key2, "approve", &[]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("Completed document docs/intro.md. You are now in idle."),
        "{out}"
    );
    assert!(
        out.contains("Action failed: REVIEW transition actions[0] command:"),
        "{out}"
    );

    // Instance and session views.
    let o = w.run(&["instance", "list"]);
    assert!(o.stdout.contains("showcase  docs/intro.md"), "{}", o.stdout);
    assert!(o.stdout.contains("completed"));
    let o = w.run(&["instance", "show", "docs/intro.md"]);
    assert!(
        o.stdout.contains("history:") && o.stdout.contains("named"),
        "{}",
        o.stdout
    );
    let o = w.run(&["instance", "show", "docs/intro.md", "--json"]);
    let v: Value = serde_json::from_str(&o.stdout).unwrap();
    assert!(v["history"].as_array().unwrap().len() > 8);
    assert_eq!(w.run(&["instance", "show", "nope"]).code, 2);
    let o = w.run(&["session", "list"]);
    assert!(o.stdout.contains(&key) && o.stdout.contains(&key2));
    let o = w.run(&["session", "show", &key2, "--json"]);
    let v: Value = serde_json::from_str(&o.stdout).unwrap();
    assert!(v["location"]["machine"].is_null());
    assert!(w.project.join(".smllm/state/.gitignore").is_file());
    // Unknown key: exit 2 with a hint.
    let o = w.run(&["fire", "--session", "sm-nope", "park"]);
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("no smllm session sm-nope"));
    let o = w.run(&["fire", "--session", &key, "enter", "--param", "oops"]);
    assert_eq!(o.code, 2);
}

// @zen-test: HOST-8_AC-1
#[test]
fn no_config_means_silent_hooks() {
    let w = World::new("noconfig");
    for hook in ["session-start", "user-prompt-submit", "stop"] {
        assert_eq!(
            w.hook(hook, json!({ "session_id": "x", "cwd": w.project })),
            json!({})
        );
    }
    let o = w.run_stdin(&["harness", "hook", "claude", "nope"], "{}");
    assert_eq!(o.code, 1);
    assert!(o.stdout.is_empty() && o.stderr.starts_with("error: "));
    assert_eq!(w.run(&["harness", "hook", "cursor", "stop"]).code, 1);
}

// @zen-test: HOST-3_AC-1
#[test]
fn fire_without_a_session_binds_one_with_enter() {
    let w = World::showcase("nokey");
    let o = w.run(&[
        "fire",
        "enter",
        "--param",
        "stateMachine=showcase",
        "--json",
    ]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    let v: Value = serde_json::from_str(&o.stdout).unwrap();
    assert!(v["session"].as_str().unwrap().starts_with("sm-"));
    assert_eq!(v["location"]["state"], "DRAFT");
    assert_eq!(w.run(&["fire", "park"]).code, 2);
}

// @zen-test: HOST-12_AC-1
// @zen-test: CLI-9_AC-1
// @zen-test: CFG-16_AC-1
#[test]
fn mcp_over_stdio() {
    let w = World::showcase("mcp");
    let key = start(&w, "cc-9");
    let mut child = Command::new(assert_cmd::cargo::cargo_bin("smllm"))
        .arg("mcp")
        .current_dir(&w.project)
        .env("XDG_STATE_HOME", w.root.join("state"))
        .env("XDG_CONFIG_HOME", w.root.join("config"))
        .env("HOME", w.root.join("home"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    struct Rpc {
        stdin: std::process::ChildStdin,
        lines: std::io::Lines<BufReader<std::process::ChildStdout>>,
    }
    impl Rpc {
        fn call(&mut self, id: u64, method: &str, params: Value) -> Value {
            if method.starts_with("notifications/") {
                writeln!(
                    self.stdin,
                    "{}",
                    json!({ "jsonrpc": "2.0", "method": method })
                )
                .unwrap();
                return Value::Null;
            }
            let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
            writeln!(self.stdin, "{msg}").unwrap();
            self.stdin.flush().unwrap();
            loop {
                let line = self.lines.next().unwrap().unwrap();
                let v: Value = serde_json::from_str(&line).unwrap();
                if v["id"] == id {
                    return v;
                }
            }
        }
    }
    let mut rpc_state = Rpc {
        stdin: child.stdin.take().unwrap(),
        lines: BufReader::new(child.stdout.take().unwrap()).lines(),
    };
    let mut rpc = |id: u64, method: &str, params: Value| rpc_state.call(id, method, params);
    let init = rpc(
        1,
        "initialize",
        json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } }),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "smllm");
    rpc(0, "notifications/initialized", Value::Null);
    let tools = rpc(2, "tools/list", json!({}));
    let tool = &tools["result"]["tools"][0];
    assert_eq!(tool["name"], "smllm");
    assert!(
        tool["description"]
            .as_str()
            .unwrap()
            .contains("Only the main agent calls smllm")
    );
    assert!(
        tool["inputSchema"]["required"].is_null(),
        "session is optional for a keyless enter (HOST-3)"
    );
    let call = |rpc: &mut dyn FnMut(u64, &str, Value) -> Value, id, args: Value| {
        let r = rpc(
            id,
            "tools/call",
            json!({ "name": "smllm", "arguments": args }),
        );
        (
            r["result"]["isError"] == true,
            r["result"]["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        )
    };
    let (err, text) = call(&mut rpc, 3, json!({ "session": key }));
    assert!(!err && text.contains("· idle"), "{text}");
    let (err, text) = call(
        &mut rpc,
        4,
        json!({ "session": key, "event": "enter", "params": { "stateMachine": "showcase" } }),
    );
    assert!(!err && text.contains("› DRAFT"), "{text}");
    let (err, text) = call(
        &mut rpc,
        5,
        json!({ "session": key, "event": "submit", "params": { "summary": 3 } }),
    );
    assert!(
        err && text.contains("param summary must be a string"),
        "{text}"
    );
    let (err, text) = call(&mut rpc, 6, json!({ "session": key, "event": "bogus" }));
    assert!(err && text.contains("bogus is not offered here"), "{text}");
    let (err, text) = call(&mut rpc, 7, json!({ "session": "sm-nope" }));
    assert!(err && text.contains("no smllm session"), "{text}");
    let (err, text) = call(
        &mut rpc,
        9,
        json!({ "session": key, "event": "park", "params": [] }),
    );
    assert!(err && text.contains("params must be an object"), "{text}");
    let (err, text) = call(&mut rpc, 10, json!({ "session": 7 }));
    assert!(err && text.contains("session must be a string"), "{text}");
    let (err, text) = call(
        &mut rpc,
        11,
        json!({ "event": "enter", "params": { "stateMachine": "showcase" } }),
    );
    assert!(
        !err && text.contains("› DRAFT"),
        "no session + enter binds one: {text}"
    );
    let r = rpc(8, "tools/call", json!({ "name": "other", "arguments": {} }));
    assert!(r["error"].is_object());
    // Close stdin so the server exits on its own (and writes coverage data).
    drop(rpc_state);
    let status = child.wait().unwrap();
    assert!(status.success(), "{status}");
}

// @zen-test: INST-10_AC-1
// @zen-test: IDLE-6_AC-1
#[test]
fn the_dev_example_through_final_and_reopen() {
    let w = World::new("dev");
    common::copy_dir(
        &common::repo().join("examples/dev"),
        &w.project.join(".smllm"),
    );
    let key = start(&w, "cc-dev");
    // Jump straight to an entry point with a ref.
    let (code, out) = fire(
        &w,
        &key,
        "enter",
        &["stateMachine=dev", "issueId=GH-3", "state=REVIEW"],
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("dev › REVIEW · issue GH-3"), "{out}");
    // approve: its command action fails here (no gh), which never blocks.
    let (code, out) = fire(&w, &key, "approve", &[]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("Completed issue GH-3."), "{out}");
    // Completed: enter without a state is refused; with an entry point it reopens.
    let (code, out) = fire(&w, &key, "enter", &["stateMachine=dev", "issueId=GH-3"]);
    assert_eq!(code, 1);
    assert!(out.contains("is completed; to reopen it"), "{out}");
    let (code, out) = fire(
        &w,
        &key,
        "enter",
        &["stateMachine=dev", "issueId=GH-3", "state=TRIAGE"],
    );
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("Reopened issue GH-3 at TRIAGE.")
            && out.contains("enter (reopened from DONE)"),
        "{out}"
    );
    // A guard that fails (no gh here) routes CHECK back to TRIAGE.
    let (_, out) = fire(&w, &key, "accept", &[]);
    assert!(
        out.contains("dev › TRIAGE (visit 2)") && out.contains("via CHECK"),
        "{out}"
    );
    let o = w.run(&["instance", "show", "GH-3"]);
    assert!(o.stdout.contains("status: active"), "{}", o.stdout);
}
