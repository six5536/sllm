//! Engine behaviour through the public protocol, with snapshots of the agent
//! text (TEST-1).

mod support;

use smllm_core::Stop;
#[allow(unused_imports)]
use smllm_core::host::Store;
use smllm_core::record::Status;
use support::*;

fn key_of(reply: &smllm_core::Reply) -> String {
    reply.session.clone()
}

// @zen-test: TURN-7_AC-1
#[test]
fn bind_starts_in_idle_with_the_idle_list() {
    let (e, mut f) = (engine(), fake());
    let r = f.bind(&e, Some("cc-1"));
    assert!(r.ok);
    assert!(r.location.machine.is_none());
    insta::assert_snapshot!(r.text);
    // The same harness session id binds the same key.
    assert_eq!(f.bind(&e, Some("cc-1")).session, r.session);
    // A new one (after /clear) gets a new key (HOST-4).
    assert_ne!(f.bind(&e, Some("cc-2")).session, r.session);
}

// @zen-test: INST-2_AC-1
// @zen-test: TURN-1_AC-1
#[test]
fn enter_a_new_instance_runs_entry_and_shows_the_header() {
    let (e, mut f) = (engine(), fake());
    let k = key_of(&f.bind(&e, None));
    let r = f.fire(&e, &k, "enter", &[("stateMachine", "dev")]);
    assert!(r.ok, "{}", r.text);
    assert_eq!(r.location.state.as_deref(), Some("TRIAGE"));
    assert!(r.location.instance.as_deref().unwrap().starts_with("i-"));
    insta::assert_snapshot!(r.text);
}

// @zen-test: TURN-2_AC-1
#[test]
fn the_stop_hook_blocks_with_the_events_list_until_yield() {
    let (e, mut f) = (engine(), fake());
    let k = key_of(&f.bind(&e, None));
    assert_eq!(f.stop(&e, &k, false), Stop::Allow, "idle may stop");
    f.fire(
        &e,
        &k,
        "enter",
        &[
            ("stateMachine", "dev"),
            ("issueId", "GH-1"),
            ("state", "REVIEW"),
        ],
    );
    let Stop::Block(text) = f.stop(&e, &k, false) else {
        panic!("expected block")
    };
    insta::assert_snapshot!(text);
    assert!(matches!(f.stop(&e, &k, true), Stop::Runaway(_)));
    let y = f.fire(&e, &k, "yield", &[("note", "asking the user")]);
    assert!(y.ok);
    assert!(y.text.contains("Yielded: staying in REVIEW"));
    assert_eq!(f.stop(&e, &k, false), Stop::Allow);
    // A user prompt clears the flag (TURN-8).
    f.with(|h| e.prompt_submitted(h, &k).unwrap());
    assert!(matches!(f.stop(&e, &k, false), Stop::Block(_)));
    // Unknown sessions may always stop.
    assert_eq!(f.stop(&e, "sm-nope", false), Stop::Allow);
}

// @zen-test: INST-3_AC-1
// @zen-test: IDLE-5_AC-1
// @zen-test: INST-11_AC-1
// @zen-test: TURN-11_AC-1
#[test]
fn set_ref_then_guarded_transitions_and_always() {
    let (e, mut f) = (engine(), fake());
    let k = key_of(&f.bind(&e, None));
    f.fire(&e, &k, "enter", &[("stateMachine", "dev")]);
    // Pattern enforced.
    let bad = f.fire(&e, &k, "issueCreated", &[("issueId", "nope")]);
    assert!(!bad.ok);
    assert!(bad.text.contains("must match"), "{}", bad.text);
    let r = f.fire(&e, &k, "issueCreated", &[("issueId", "GH-7")]);
    assert!(r.ok, "{}", r.text);
    assert_eq!(r.location.r#ref.as_deref(), Some("GH-7"));
    insta::assert_snapshot!("arrive_in_work", r.text);
    // Entry command saw the ref.
    let ran = f.ran.borrow().clone();
    let (_, run, env) = ran.last().unwrap();
    assert_eq!(run, "git switch issue");
    assert!(env.contains(&("SMLLM_REF".into(), "GH-7".into())));
    assert!(env.contains(&("SMLLM_TO".into(), "WORK".into())));
    assert!(env.contains(&("SMLLM_PARAM_ISSUE_ID".into(), "GH-7".into())));

    // Tests fail → self re-entry, visit 2.
    f.guard_results.insert("cargo test --quiet".into(), false);
    let r = f.fire(&e, &k, "submit", &[("summary", "fix")]);
    assert_eq!(r.location.state.as_deref(), Some("WORK"));
    insta::assert_snapshot!("reenter_work", r.text);
    // Third visit → escalate.
    f.fire(&e, &k, "submit", &[("summary", "fix")]);
    let r = f.fire(&e, &k, "submit", &[("summary", "fix")]);
    assert_eq!(r.location.state.as_deref(), Some("ESCALATE"), "{}", r.text);
    f.fire(&e, &k, "resolved", &[]);
    f.guard_results.insert("cargo test --quiet".into(), true);
    let r = f.fire(&e, &k, "submit", &[("summary", "fix \"it\"")]);
    assert_eq!(r.location.state.as_deref(), Some("REVIEW"));
    let ran = f.ran.borrow().clone();
    assert!(ran.iter().any(|(_, run, env)| run.starts_with("git commit")
        && env.contains(&("SMLLM_PARAM_SUMMARY".into(), "fix \"it\"".into()))));
    // Final: completed, idle, idle list follows.
    let r = f.fire(&e, &k, "approve", &[]);
    assert!(r.ok);
    assert!(r.location.machine.is_none());
    insta::assert_snapshot!("final", r.text);
    let inst = f.with(|h| h.store.instances("dev").unwrap()).pop().unwrap();
    assert_eq!(inst.status, Status::Completed);
    assert_eq!(inst.visits("WORK"), 4);
    // setRef twice is an error — reopen then fire issueCreated again.
    let r = f.fire(
        &e,
        &k,
        "enter",
        &[
            ("stateMachine", "dev"),
            ("issueId", "GH-7"),
            ("state", "TRIAGE"),
        ],
    );
    assert!(r.ok, "{}", r.text);
    assert!(r.text.contains("Reopened issue GH-7 at TRIAGE."));
    // Visits and history carry on across the reopen (INST-11).
    let inst = f.with(|h| h.store.instances("dev").unwrap()).pop().unwrap();
    assert_eq!(inst.visits("WORK"), 4);
    assert_eq!(inst.visits("TRIAGE"), 2);
    assert!(f.store.history.iter().next().unwrap().1.len() > 8);
    let r = f.fire(&e, &k, "issueCreated", &[("issueId", "GH-8")]);
    assert!(!r.ok);
    assert!(r.text.contains("already has its ref GH-7"), "{}", r.text);
}

#[test]
fn always_state_routes_by_guard() {
    let (e, mut f) = (engine(), fake());
    let k = key_of(&f.bind(&e, None));
    f.fire(
        &e,
        &k,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-2")],
    );
    f.guard_results
        .insert("gh issue view \"$SMLLM_REF\"".into(), false);
    let r = f.fire(&e, &k, "accept", &[]);
    assert_eq!(r.location.state.as_deref(), Some("TRIAGE"), "{}", r.text);
    assert!(
        r.text.contains("Arrived by: accept from TRIAGE via CHECK"),
        "{}",
        r.text
    );
    f.guard_results
        .insert("gh issue view \"$SMLLM_REF\"".into(), true);
    let r = f.fire(&e, &k, "accept", &[]);
    assert_eq!(r.location.state.as_deref(), Some("WORK"));
}

// @zen-test: TURN-3_AC-1
#[test]
fn invalid_calls_change_nothing() {
    let (e, mut f) = (engine(), fake());
    let k = key_of(&f.bind(&e, None));
    f.fire(
        &e,
        &k,
        "enter",
        &[
            ("stateMachine", "dev"),
            ("issueId", "GH-3"),
            ("state", "REVIEW"),
        ],
    );
    let before = f.store.clone();
    for (ev, ps) in [
        ("frobnicate", vec![]),
        ("reject", vec![]),
        ("reject", vec![("reason", "x"), ("severity", "huge")]),
        ("reject", vec![("reason", "x"), ("bogus", "y")]),
        ("park", vec![("x", "y")]),
    ] {
        let r = f.fire(&e, &k, ev, &ps);
        assert!(!r.ok, "{ev}");
        assert!(r.text.contains("error: "), "{}", r.text);
    }
    let r = f.fire(&e, &k, "reject", &[("reason", "x"), ("severity", "huge")]);
    insta::assert_snapshot!("error_block", r.text);
    assert_eq!(f.store, before);
}

// @zen-test: IDLE-6_AC-1
#[test]
fn park_and_enter_again() {
    let (e, mut f) = (engine(), fake());
    let k = key_of(&f.bind(&e, None));
    f.fire(
        &e,
        &k,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-4")],
    );
    let r = f.fire(&e, &k, "park", &[]);
    assert!(r.location.machine.is_none());
    assert!(r.text.contains("Parked issue GH-4 at TRIAGE."));
    assert!(
        r.text.contains("Parked:\n- issue GH-4 (dev) at TRIAGE"),
        "{}",
        r.text
    );
    let r = f.fire(
        &e,
        &k,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-4")],
    );
    assert!(r.text.contains("(visit 2)"), "{}", r.text);
    assert!(r.text.contains("Arrived by: enter\n"));
    // Jump to an entry point.
    f.fire(&e, &k, "park", &[]);
    let r = f.fire(
        &e,
        &k,
        "enter",
        &[
            ("stateMachine", "dev"),
            ("issueId", "GH-4"),
            ("state", "REVIEW"),
        ],
    );
    assert!(
        r.text.contains("Arrived by: enter (jump from TRIAGE)"),
        "{}",
        r.text
    );
    // Not an entry point.
    f.fire(&e, &k, "park", &[]);
    let r = f.fire(
        &e,
        &k,
        "enter",
        &[
            ("stateMachine", "dev"),
            ("issueId", "GH-4"),
            ("state", "WORK"),
        ],
    );
    assert!(!r.ok);
    assert!(r.text.contains("WORK is not an entry point of dev"));
}

// @zen-test: INST-6_AC-1
// @zen-test: INST-7_AC-1
#[test]
fn a_second_session_takes_over_and_the_first_is_told() {
    let (e, mut f) = (engine(), fake());
    let a = key_of(&f.bind(&e, Some("a")));
    let b = key_of(&f.bind(&e, Some("b")));
    f.fire(
        &e,
        &a,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-5")],
    );
    let r = f.fire(
        &e,
        &b,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-5")],
    );
    assert!(
        r.text.contains(&format!(
            "Took over from session {a} (last active 2026-09-25 10:32 UTC)"
        )),
        "{}",
        r.text
    );
    let r = f.fire(&e, &a, "accept", &[]);
    assert!(!r.ok);
    assert!(
        r.text.contains(&format!(
            "error: issue GH-5 moved to session {b}; you are in idle"
        )),
        "{}",
        r.text
    );
    assert!(r.location.machine.is_none());
}

// @zen-test: IDLE-2_AC-1
#[test]
fn unmatched_detours_via_idle_or_the_fallback_state() {
    let (e, mut f) = (engine(), fake());
    let k = key_of(&f.bind(&e, None));
    f.fire(
        &e,
        &k,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-6")],
    );
    let r = f.fire(&e, &k, "unmatched", &[]);
    assert!(
        r.text.contains("Suspended: issue GH-6 (dev) at TRIAGE"),
        "{}",
        r.text
    );
    assert!(
        r.text
            .contains("- resume — Return to issue GH-6 (dev) at TRIAGE.")
    );
    // Detour: another instance, finished, then resume.
    f.fire(&e, &k, "enter", &[("stateMachine", "help")]);
    let r = f.fire(&e, &k, "answered", &[]);
    assert!(r.text.contains("Completed instance"), "{}", r.text);
    let r = f.fire(&e, &k, "resume", &[]);
    assert!(r.ok, "{}", r.text);
    assert!(r.text.contains("Arrived by: resume"));
    assert!(r.text.contains("(visit 2)"));

    // Fallback state: unmatched goes there, resume returns.
    f.fire(&e, &k, "park", &[]);
    f.fire(&e, &k, "enter", &[("stateMachine", "help")]);
    let r = f.fire(&e, &k, "unmatched", &[]);
    assert_eq!(r.location.state.as_deref(), Some("ASIDE"), "{}", r.text);
    let menu = f.with(|h| e.menu(h, &k).unwrap());
    assert!(
        menu.text.contains("- resume — Return to ASK."),
        "{}",
        menu.text
    );
    assert!(menu.text.contains("handle it from idle"), "{}", menu.text);
    let r = f.fire(&e, &k, "resume", &[]);
    assert_eq!(r.location.state.as_deref(), Some("ASK"));
    // A second suspend parks the first suspended instance.
    f.fire(&e, &k, "park", &[]);
    f.fire(
        &e,
        &k,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-6")],
    );
    f.fire(&e, &k, "unmatched", &[]);
    f.fire(
        &e,
        &k,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-9")],
    );
    let r = f.fire(&e, &k, "unmatched", &[]);
    assert!(
        r.text.contains("Parked the previously suspended GH-6."),
        "{}",
        r.text
    );
}

// @zen-test: ENG-5_AC-1
#[test]
fn view_is_read_only() {
    let (e, mut f) = (engine(), fake());
    let k = key_of(&f.bind(&e, None));
    f.fire(
        &e,
        &k,
        "enter",
        &[
            ("stateMachine", "dev"),
            ("issueId", "GH-1"),
            ("state", "REVIEW"),
        ],
    );
    let before = f.store.clone();
    let ran = f.ran.borrow().len();
    let r = f.view(&e, &k);
    insta::assert_snapshot!(r.text);
    assert_eq!(f.store, before);
    assert_eq!(f.ran.borrow().len(), ran);
}

// @zen-test: ACT-3_AC-1
#[test]
fn failed_actions_are_reported_and_never_block() {
    let (e, mut f) = (engine(), fake());
    f.failing_actions.push("git switch issue".into());
    f.files.remove("work.md");
    let k = key_of(&f.bind(&e, None));
    f.fire(
        &e,
        &k,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-1")],
    );
    let r = f.fire(&e, &k, "accept", &[]);
    assert_eq!(r.location.state.as_deref(), Some("WORK"));
    assert!(
        r.text
            .contains("Action failed: WORK entry[0] command: exited 128: boom"),
        "{}",
        r.text
    );
    assert!(
        r.text.contains("Prompt failed: work.md: not found"),
        "{}",
        r.text
    );
}

// @zen-test: INST-10_AC-1
#[test]
fn completed_instances_need_a_state_to_reopen() {
    let (e, mut f) = (engine(), fake());
    let k = key_of(&f.bind(&e, None));
    f.fire(
        &e,
        &k,
        "enter",
        &[
            ("stateMachine", "dev"),
            ("issueId", "GH-1"),
            ("state", "REVIEW"),
        ],
    );
    f.fire(&e, &k, "approve", &[]);
    let r = f.fire(
        &e,
        &k,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-1")],
    );
    assert!(!r.ok);
    assert!(
        r.text
            .contains("is completed; to reopen it, fire enter with state: one of TRIAGE, REVIEW"),
        "{}",
        r.text
    );
}

// @zen-test: IDLE-3_AC-1
#[test]
fn a_missing_saved_state_is_repaired_by_enter_with_any_state() {
    let (e, mut f) = (engine(), fake());
    let k = key_of(&f.bind(&e, None));
    f.fire(
        &e,
        &k,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-1")],
    );
    f.fire(&e, &k, "park", &[]);
    // Config changed under the saved instance.
    f.with(|h| {
        let mut i = h.store.instances("dev").unwrap().pop().unwrap();
        i.state = "GONE".into();
        i.version += 1;
        h.store.put_instance(&i).unwrap();
    });
    let r = f.fire(
        &e,
        &k,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-1")],
    );
    assert!(!r.ok);
    assert!(
        r.text
            .contains("saved state GONE of issue GH-1 no longer exists"),
        "{}",
        r.text
    );
    let r = f.fire(
        &e,
        &k,
        "enter",
        &[
            ("stateMachine", "dev"),
            ("issueId", "GH-1"),
            ("state", "WORK"),
        ],
    );
    assert!(r.ok, "{}", r.text);
    assert!(r.text.contains("enter (saved state GONE no longer exists)"));
}

#[test]
fn idle_rejects_unknown_events_and_params() {
    let (e, mut f) = (engine(), fake());
    let k = key_of(&f.bind(&e, None));
    for (ev, ps, msg) in [
        (
            "park",
            vec![],
            "park is not offered in idle (offered: enter)",
        ),
        ("enter", vec![], "enter needs param stateMachine"),
        (
            "enter",
            vec![("stateMachine", "nope")],
            "must be one of: dev, help",
        ),
        (
            "enter",
            vec![("stateMachine", "dev"), ("ref", "x")],
            "enter has no param ref for dev",
        ),
        (
            "enter",
            vec![("stateMachine", "dev"), ("issueId", "x")],
            "must match",
        ),
        (
            "enter",
            vec![("stateMachine", "dev"), ("state", "WORK")],
            "not an entry point",
        ),
        ("resume", vec![], "resume is not offered in idle"),
    ] {
        let r = f.fire(&e, &k, ev, &ps);
        assert!(!r.ok);
        assert!(r.text.contains(msg), "{ev}: {}", r.text);
    }
}

#[test]
fn unknown_sessions_are_errors_with_a_hint() {
    let e = engine();
    let mut f = fake();
    let err = f.with(|h| e.view(h, "sm-zzz").unwrap_err());
    assert!(err.to_string().contains("no smllm session sm-zzz"));
    let err = f.with(|h| {
        e.fire(h, None, "park", &[], &smllm_core::Bind::default())
            .unwrap_err()
    });
    assert_eq!(err, smllm_core::Error::MissingSession);
    // No key + enter binds a new session (HOST-3).
    let r = f.with(|h| {
        e.fire(
            h,
            None,
            "enter",
            &[("stateMachine".into(), "help".into())],
            &smllm_core::Bind {
                harness: "none",
                ..Default::default()
            },
        )
        .unwrap()
    });
    assert!(r.ok);
    assert!(r.session.starts_with("sm-"));
}

// @zen-test: NFR-9_AC-1
#[test]
fn unsupported_kinds_are_listed() {
    let e = engine();
    let mut f = fake();
    f.no_host_kinds = true;
    let found = f.with(|h| e.unsupported(h.guards, h.actions));
    assert!(
        found.contains(&"dev.WORK: guard type command is not supported by this host".to_string()),
        "{found:?}"
    );
    assert!(
        found.contains(&"dev.WORK: action type command is not supported by this host".to_string())
    );
    let mut f = fake();
    assert!(f.with(|h| e.unsupported(h.guards, h.actions)).is_empty());
}

// Regressions from the implementation double-check.

// @zen-test: IDLE-2_AC-1
#[test]
fn the_fallback_state_keeps_its_way_back_across_a_suspend() {
    let (e, mut f) = (engine(), fake());
    let k = key_of(&f.bind(&e, None));
    f.fire(&e, &k, "enter", &[("stateMachine", "help")]);
    let r = f.fire(&e, &k, "unmatched", &[]);
    assert_eq!(r.location.state.as_deref(), Some("ASIDE"));
    // unmatched in the fallback state suspends to idle; resume comes back
    // to ASIDE, which still offers resume → ASK.
    f.fire(&e, &k, "unmatched", &[]);
    let r = f.fire(&e, &k, "resume", &[]);
    assert_eq!(r.location.state.as_deref(), Some("ASIDE"), "{}", r.text);
    let r = f.fire(&e, &k, "resume", &[]);
    assert_eq!(r.location.state.as_deref(), Some("ASK"), "{}", r.text);
}

// @zen-test: INST-7_AC-1
#[test]
fn the_stop_hook_tells_a_superseded_session_once() {
    let (e, mut f) = (engine(), fake());
    let a = key_of(&f.bind(&e, Some("a")));
    let b = key_of(&f.bind(&e, Some("b")));
    f.fire(
        &e,
        &a,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-5")],
    );
    f.fire(
        &e,
        &b,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-5")],
    );
    let Stop::Block(text) = f.stop(&e, &a, false) else {
        panic!("expected block")
    };
    assert!(text.contains(&format!("moved to session {b}")), "{text}");
    assert_eq!(
        f.stop(&e, &a, false),
        Stop::Allow,
        "reported once, then idle"
    );
}

// @zen-test: ENG-5_AC-1
#[test]
fn a_view_never_drops_the_session_even_if_its_machine_is_missing() {
    let (e, mut f) = (engine(), fake());
    let k = key_of(&f.bind(&e, None));
    f.fire(
        &e,
        &k,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-1")],
    );
    // The config briefly lacks `dev` (e.g. an invalid file).
    let broken = smllm_core::Engine::new(smllm_core::model::Config {
        machines: vec![helpdesk()],
        idle: vec![],
    });
    let before = f.store.clone();
    let r = f.view(&broken, &k);
    assert!(
        r.text.contains("state machine dev is not configured"),
        "{}",
        r.text
    );
    assert_eq!(f.stop(&broken, &k, false), Stop::Allow);
    let r = f.fire(&broken, &k, "park", &[]);
    assert!(!r.ok);
    assert_eq!(f.store, before, "nothing written");
    // Fixed config: the session is still where it was.
    assert_eq!(f.view(&e, &k).location.state.as_deref(), Some("TRIAGE"));
}

#[test]
fn the_suspended_slot_never_parks_another_sessions_instance() {
    let (e, mut f) = (engine(), fake());
    let a = key_of(&f.bind(&e, Some("a")));
    let b = key_of(&f.bind(&e, Some("b")));
    f.fire(
        &e,
        &a,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-1")],
    );
    f.fire(&e, &a, "unmatched", &[]);
    // b takes GH-1 over and suspends it itself.
    f.fire(
        &e,
        &b,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-1")],
    );
    f.fire(&e, &b, "unmatched", &[]);
    // a's idle list no longer shows GH-1 as its suspended instance.
    let r = f.view(&e, &a);
    assert!(!r.text.contains("Suspended:"), "{}", r.text);
    f.fire(
        &e,
        &a,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-2")],
    );
    let r = f.fire(&e, &a, "unmatched", &[]);
    assert!(
        !r.text.contains("Parked the previously suspended"),
        "{}",
        r.text
    );
    let r = f.fire(&e, &b, "resume", &[]);
    assert!(r.ok, "{}", r.text);
}

#[test]
fn a_rejected_keyless_enter_leaves_no_session() {
    let e = engine();
    let mut f = fake();
    let r = f.with(|h| {
        e.fire(
            h,
            None,
            "enter",
            &[("stateMachine".into(), "nope".into())],
            &smllm_core::Bind {
                harness: "none",
                ..Default::default()
            },
        )
        .unwrap()
    });
    assert!(!r.ok);
    assert!(f.store.sessions.is_empty());
}
