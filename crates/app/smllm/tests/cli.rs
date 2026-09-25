//! End-to-end CLI tests: the real binary, its output, JSON and exit codes.

mod common;

use common::{World, repo};

#[test]
fn version_help_and_usage_errors() {
    let w = World::new("basics");
    for flag in ["-V", "--version"] {
        let o = w.run(&[flag]);
        assert_eq!(o.code, 0);
        assert_eq!(
            o.stdout.trim(),
            format!("smllm {}", env!("CARGO_PKG_VERSION"))
        );
    }
    let o = w.run(&[]);
    assert!(o.stdout.contains("Usage: smllm"), "{}", o.stdout);
    assert!(o.stdout.contains("Exit codes: 0 success, 1 errors found"));
    let o = w.run(&["--help"]);
    for cmd in [
        "init",
        "new",
        "validate",
        "fire",
        "session",
        "instance",
        "harness",
        "graph",
        "info",
        "compile",
        "completions",
    ] {
        assert!(o.stdout.contains(cmd), "{cmd}: {}", o.stdout);
    }
    assert!(!o.stdout.contains("\n  mcp") && !o.stdout.contains("\n  man"));
    for args in [
        vec!["--definitely-not-a-flag"],
        vec!["frobnicate"],
        vec!["completions"],
        vec!["fire"],
    ] {
        assert_eq!(w.run(&args).code, 2, "{args:?}");
    }
}

// @zen-test: CLI-12_AC-1
#[test]
fn completions_and_man() {
    let w = World::new("docs");
    for (shell, marker) in [
        ("bash", "_smllm()"),
        ("zsh", "#compdef smllm"),
        ("fish", "complete -c smllm"),
    ] {
        let o = w.run(&["completions", shell]);
        assert_eq!(o.code, 0);
        assert!(o.stdout.contains(marker), "{shell}");
    }
    let o = w.run(&["man"]);
    assert!(o.stdout.contains(".TH smllm 1"));
    assert!(
        o.stdout.contains(".SH COMMANDS")
            && o.stdout.contains(".SH EXIT STATUS")
            && o.stdout.contains(".SH FILES")
    );
    assert!(
        o.stdout
            .contains(".SS \"smllm validate <PATHS> [OPTIONS]\""),
        "{}",
        o.stdout
    );
}

// @zen-test: CLI-1_AC-1
// @zen-test: CLI-2_AC-1
// @zen-test: CLI-3_AC-1
#[test]
fn init_new_validate() {
    let w = World::new("init");
    assert_eq!(w.run(&["validate"]).code, 2, "no config yet");
    let o = w.run(&["init"]);
    assert_eq!(o.stdout, "created .smllm/config.toml\n");
    assert!(w.run(&["init"]).stdout.contains("already exists"));
    let o = w.run(&["init", "--user", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&o.stdout).unwrap();
    assert_eq!(v["created"], true);
    let o = w.run(&["new", "demo"]);
    assert!(o.stdout.contains("id: demo"));
    assert_eq!(w.run(&["new", "bad id"]).code, 2);
    let o = w.run(&["new", "demo", "--write"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(
        w.read(".smllm/config.toml")
            .contains("files = [\"demo.smllm.yaml\"]")
    );
    assert_eq!(w.run(&["new", "demo", "--write"]).code, 2, "exists");
    let o = w.run(&["validate"]);
    assert_eq!(
        (o.code, o.stdout.as_str()),
        (0, "0 errors, 0 warnings, 0 info\n")
    );

    // Break it: errors are listed with file:line and exit 1.
    let broken = w
        .read(".smllm/demo.smllm.yaml")
        .replace("done: DONE", "done: NOWHERE");
    w.write(".smllm/demo.smllm.yaml", &broken);
    let o = w.run(&["validate", "--warnings"]);
    assert_eq!(o.code, 1);
    assert!(o.stdout.contains(".smllm/demo.smllm.yaml:"), "{}", o.stdout);
    assert!(
        o.stdout
            .contains("error: states.START.on.done: target state NOWHERE does not exist (CFG-3)"),
        "{}",
        o.stdout
    );
    let o = w.run(&["validate", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&o.stdout).unwrap();
    assert_eq!(v["errors"].as_array().unwrap().len(), 1);
    // Paths: a machine file directly.
    let o = w.run(&["validate", ".smllm/demo.smllm.yaml"]);
    assert_eq!(o.code, 1);
}

// @zen-test: CLI-11_AC-1
#[test]
fn examples_validate_schema_compile() {
    let w = World::new("examples");
    let showcase = repo().join("examples/showcase/config.toml");
    let s = showcase.to_str().unwrap();
    let o = w.run(&["--config", s, "validate", "--info"]);
    assert_eq!(o.code, 0, "{}", o.stdout);
    let o = w.run(&[
        "validate",
        repo().join("examples/dev/config.toml").to_str().unwrap(),
    ]);
    assert_eq!(o.code, 0, "{}", o.stdout);
    let o = w.run(&["info", "schema"]);
    let v: serde_json::Value = serde_json::from_str(&o.stdout).unwrap();
    assert_eq!(v["title"], "smllm state machine");
    let o = w.run(&["compile", s]);
    assert!(o.stdout.contains("Write the draft."));
    let o = w.run(&["compile", s, "-o", "out.json"]);
    assert_eq!(o.code, 0);
    assert!(w.read("out.json").starts_with('{'));
    w.write("bad.smllm.yaml", "id: x\n");
    let o = w.run(&["compile", "bad.smllm.yaml"]);
    assert_eq!(o.code, 1);
    assert!(o.stderr.contains("error"), "{}", o.stderr);
}

// @zen-test: CLI-10_AC-1
#[test]
fn graph_forms() {
    let w = World::showcase("graph");
    let o = w.run(&["graph"]);
    assert!(o.stdout.contains("showcase (initial DRAFT)"));
    assert!(
        o.stdout
            .contains("always [command `test -s \"$SMLLM_REF\"`] → REVIEW"),
        "{}",
        o.stdout
    );
    let o = w.run(&["graph", "showcase", "--mermaid"]);
    assert!(o.stdout.contains("stateDiagram-v2") && o.stdout.contains("DONE --> [*]"));
    let o = w.run(&["graph", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&o.stdout).unwrap();
    assert_eq!(v["machines"][0]["id"], "showcase");
    assert_eq!(w.run(&["graph", "nope"]).code, 2);
}

// @zen-test: HOST-6_AC-1
// @zen-test: CLI-7_AC-1
#[test]
fn harness_install_and_status() {
    let w = World::showcase("harness");
    let o = w.run(&["harness", "status", "claude"]);
    assert!(
        o.stdout.contains("absent  CLAUDE.md (instructions)"),
        "{}",
        o.stdout
    );
    let o = w.run(&["harness", "install", "claude"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(w.read("CLAUDE.md").contains("<!-- smllm:harness -->"));
    assert!(w.read(".mcp.json").contains("\"smllm\""));
    let settings = w.read(".claude/settings.json");
    assert!(
        settings.contains("smllm harness hook claude stop")
            && settings.contains("mcp__smllm__smllm")
    );
    let o = w.run(&["harness", "status", "claude", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&o.stdout).unwrap();
    assert!(
        v["parts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["state"] == "current"),
        "{v}"
    );
    // AGENTS.md alone → it gets the block (HOST-10).
    let w2 = World::showcase("harness2");
    w2.write("AGENTS.md", "# Agents\n");
    w2.run(&["harness", "install", "claude", "--without", "mcp"]);
    assert!(w2.read("AGENTS.md").contains("smllm:harness"));
    assert!(!w2.project.join(".mcp.json").exists());
    assert_eq!(
        w.run(&["harness", "install", "claude", "--scope", "galaxy"])
            .code,
        2
    );
    assert_eq!(w.run(&["harness", "install", "cursor"]).code, 2);
}

// @zen-test: CLI-3_AC-2
#[test]
fn new_forms_and_validate_paths() {
    let w = World::new("newforms");
    assert_eq!(
        w.run(&["new", "x", "--write"]).code,
        2,
        "no config to register in"
    );
    w.run(&["init"]);
    let o = w.run(&["new", "demo", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&o.stdout).unwrap();
    assert!(v["path"].is_null() && v["text"].as_str().unwrap().contains("id: demo"));
    std::fs::create_dir_all(w.project.join("machines")).unwrap();
    let o = w.run(&["new", "other", "--write", "--dir", "machines", "--json"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(
        w.read(".smllm/config.toml")
            .contains("../machines/other.smllm.yaml"),
        "{}",
        w.read(".smllm/config.toml")
    );
    let o = w.run(&["validate", ".smllm/config.toml", "--info"]);
    assert_eq!(o.code, 0, "{}", o.stdout);
    // A saved instance whose state vanished is a warning (IDLE-4).
    let o = w.run(&["fire", "enter", "--param", "stateMachine=other", "--json"]);
    assert_eq!(o.code, 0, "{}", o.stdout);
    let yaml = w
        .read("machines/other.smllm.yaml")
        .replace("START", "BEGIN");
    w.write("machines/other.smllm.yaml", &yaml);
    let o = w.run(&["validate", "--warnings"]);
    assert!(
        o.stdout
            .contains("is in state START, which no longer exists"),
        "{}",
        o.stdout
    );
    // An explicit config that does not exist.
    assert_eq!(w.run(&["--config", "nope.toml", "validate"]).code, 2);
    w.write(".smllm/config.toml", "[machines]\nfiles = 3\n");
    let o = w.run(&["new", "z", "--write"]);
    assert_eq!(o.code, 2, "{}", o.stdout);
}

#[test]
fn user_scope_harness_goes_through_claude_mcp() {
    let w = World::showcase("userscope");
    // A stand-in `claude` that records `mcp add-json --scope user` the way
    // Claude Code does, in ~/.claude.json.
    let bin = w.root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let home = w.root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let script = format!(
        "#!/bin/sh\nif [ \"$2\" = add-json ]; then printf '{{\"mcpServers\":{{\"smllm\":%s}}}}' \"$6\" > {}/.claude.json; fi\n",
        home.display()
    );
    let fake = bin.join("claude");
    std::fs::write(&fake, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let o = w
        .cmd()
        .args(["harness", "install", "claude", "--scope", "user"])
        .env("PATH", &path)
        .output()
        .unwrap();
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let out = String::from_utf8(o.stdout).unwrap();
    assert!(out.contains("claude mcp (user) (mcp)"), "{out}");
    assert!(
        std::fs::read_to_string(home.join(".claude/settings.json"))
            .unwrap()
            .contains("smllm harness hook claude stop")
    );
    assert!(
        std::fs::read_to_string(home.join(".claude/CLAUDE.md"))
            .unwrap()
            .contains("smllm:harness")
    );
    let o = w
        .cmd()
        .args(["harness", "status", "claude", "--scope", "user"])
        .env("PATH", &path)
        .output()
        .unwrap();
    let out = String::from_utf8(o.stdout).unwrap();
    assert!(out.lines().all(|l| l.starts_with("current")), "{out}");
    // Without the claude CLI the mcp part is a refusal.
    std::fs::remove_file(home.join(".claude.json")).unwrap();
    let o = w
        .cmd()
        .args(["harness", "install", "claude", "--scope", "user"])
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("claude"));
}

/// `schema/smllm.schema.json` (linked from `smllm new`'s template) is what
/// `smllm info schema` prints; regenerate with
/// `cargo run -p smllm -- info schema > schema/smllm.schema.json`.
// @zen-test: CFG-15_AC-1
#[test]
fn the_published_schema_is_current() {
    let w = World::new("schema");
    let printed = w.run(&["info", "schema"]).stdout;
    let committed = std::fs::read_to_string(repo().join("schema/smllm.schema.json")).unwrap();
    assert_eq!(
        printed.replace("\r\n", "\n"),
        committed.replace("\r\n", "\n")
    );
}
