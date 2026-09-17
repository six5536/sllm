//! End-to-end CLI tests: invoke the real binary and assert output, JSON, and
//! exit codes. These are the tests the release smoke scripts mirror, so they
//! are the contract the packaged binary is held to.

use assert_cmd::Command;

/// The binary under test, as a fresh command.
fn sllm() -> Command {
    Command::cargo_bin("sllm").unwrap()
}

/// Run with `args` and return (stdout, stderr, exit code).
fn run(args: &[&str]) -> (String, String, i32) {
    let out = sllm().args(args).output().unwrap();
    (
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
        out.status.code().unwrap(),
    )
}

#[test]
fn version_prints_name_and_semver() {
    for flag in ["-V", "--version"] {
        let (stdout, _, code) = run(&[flag]);
        assert_eq!(code, 0);
        assert_eq!(
            stdout.trim(),
            format!("sllm {}", env!("CARGO_PKG_VERSION")),
            "`{flag}` output"
        );
    }
}

#[test]
fn bare_invocation_prints_help_and_exits_clean() {
    let (stdout, _, code) = run(&[]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Usage: sllm"), "{stdout}");
    // The after-help pointer users need when nothing else is on screen.
    assert!(
        stdout.contains("Exit codes: 0 success, 2 error"),
        "{stdout}"
    );
}

#[test]
fn help_lists_the_visible_commands_and_hides_man() {
    let (stdout, _, code) = run(&["--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("hello"), "{stdout}");
    assert!(stdout.contains("completions"), "{stdout}");
    assert!(
        !stdout.contains("\n  man"),
        "man is hidden from help: {stdout}"
    );
}

#[test]
fn hello_greets_the_world_by_default() {
    let (stdout, stderr, code) = run(&["hello"]);
    assert_eq!(code, 0);
    assert_eq!(stdout, "Hello, world!\n");
    assert_eq!(stderr, "");
}

#[test]
fn hello_greets_a_named_recipient() {
    let (stdout, _, code) = run(&["hello", "ada"]);
    assert_eq!(code, 0);
    assert_eq!(stdout, "Hello, ada!\n");
}

#[test]
fn hello_json_emits_a_single_parsable_object() {
    let (stdout, _, code) = run(&["hello", "ada", "--json"]);
    assert_eq!(code, 0);
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(doc["name"], "ada");
    assert_eq!(doc["message"], "Hello, ada!");
}

#[test]
fn a_blank_name_fails_with_a_message_on_stderr() {
    let (stdout, stderr, code) = run(&["hello", "   "]);
    assert_eq!(code, 2, "errors exit 2");
    assert_eq!(stdout, "", "nothing on stdout for a failed run");
    assert!(stderr.starts_with("error: "), "{stderr}");
    assert!(stderr.contains("must not be blank"), "{stderr}");
}

#[test]
fn usage_errors_exit_2() {
    // The npm launcher's smoke test asserts this exact code for an unknown
    // flag, so it is part of the contract rather than a clap detail.
    for args in [
        vec!["--definitely-not-a-flag"],
        vec!["frobnicate"],
        vec!["completions"],
        vec!["completions", "cmd.exe"],
    ] {
        let (_, stderr, code) = run(&args);
        assert_eq!(code, 2, "`sllm {}` -> {stderr}", args.join(" "));
    }
}

#[test]
fn completions_render_for_every_supported_shell() {
    // A marker unique to each shell's script, so a silently empty or wrong
    // generator is caught rather than "it exited 0".
    for (shell, marker) in [
        ("bash", "_sllm()"),
        ("zsh", "#compdef sllm"),
        ("fish", "complete -c sllm"),
        ("powershell", "Register-ArgumentCompleter"),
        ("elvish", "set edit:completion:arg-completer[sllm]"),
    ] {
        let (stdout, _, code) = run(&["completions", shell]);
        assert_eq!(code, 0, "{shell} exited {code}");
        assert!(stdout.contains(marker), "{shell} script lacks {marker}");
    }
}

#[test]
fn man_renders_the_hand_written_sections() {
    let (stdout, _, code) = run(&["man"]);
    assert_eq!(code, 0);
    // clap_mangen's own sections...
    assert!(stdout.contains(".TH sllm 1"), "{stdout}");
    assert!(stdout.contains(".SH SYNOPSIS"), "{stdout}");
    // ...plus the ones man.rs appends, with each visible verb documented.
    assert!(stdout.contains(".SH COMMANDS"), "{stdout}");
    assert!(
        stdout.contains(".SS \"sllm hello <NAME> [OPTIONS]\""),
        "{stdout}"
    );
    assert!(stdout.contains(".SH EXIT STATUS"), "{stdout}");
    // The hidden verb stays out of the page, like it stays out of --help.
    assert!(!stdout.contains(".SS \"sllm man\""), "{stdout}");
}
