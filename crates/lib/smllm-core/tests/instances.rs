//! Engine edge cases around instances, idle and refs: the fixes from the
//! 2026-09 code review (PLAN-003 P4).

mod support;

use smllm_core::model::{ActionDef, Config, EventDef, InstanceSpec, Machine};
use smllm_core::{Bind, Engine, SmallMap};
use support::*;

fn key_of(reply: &smllm_core::Reply) -> String {
    reply.session.clone()
}

/// `plan`: DRAFT --written (setRef)--> GRILL --revise--> DRAFT; DRAFT also
/// has `go`, whose first branch sets the ref only from the second visit.
fn plan() -> Machine {
    let mut events = SmallMap::new();
    for name in ["written", "go"] {
        events.insert(
            name,
            EventDef {
                description: None,
                params: vec![param("planId", name == "written")],
            },
        );
    }
    let mut draft = state("DRAFT");
    draft.entry_point = true;
    draft.on = vec![
        on(
            "written",
            vec![smllm_core::model::Transition {
                actions: vec![ActionDef::SetRef],
                ..to("GRILL")
            }],
        ),
        on(
            "go",
            vec![
                smllm_core::model::Transition {
                    guard: Some(smllm_core::model::GuardDef::Visits {
                        state: "DRAFT".into(),
                        at_least: 2,
                    }),
                    actions: vec![ActionDef::SetRef],
                    ..to("GRILL")
                },
                to("GRILL"),
            ],
        ),
    ];
    let mut grill = state("GRILL");
    grill.on = vec![on("revise", vec![to("DRAFT")])];
    Machine {
        id: "plan".into(),
        description: None,
        initial: "DRAFT".into(),
        instance: InstanceSpec {
            kind: "plan".into(),
            ref_param: "planId".into(),
            ..InstanceSpec::default()
        },
        events,
        shared: vec![],
        states: vec![draft, grill],
    }
}

/// `help` with an exit prompt on ASK and a self-transition in the fallback.
fn help() -> Machine {
    let mut m = helpdesk();
    for s in &mut m.states {
        match s.name.as_str() {
            "ASK" => s.exit = vec![text("Leaving ASK: note where you were.")],
            "ASIDE" => s.on.push(on("note", vec![to("ASIDE")])),
            _ => {}
        }
    }
    m
}

fn engine2() -> Engine {
    Engine::new(Config {
        machines: vec![dev(), help(), plan()],
        idle: vec![],
    })
}

// Setting the ref the instance already has is a no-op, so a machine can loop
// back through a ref-setting transition (D3-3); another value is refused.
// @zen-test: INST-3_AC-1
#[test]
fn setting_the_same_ref_again_is_a_no_op() {
    let (e, mut f) = (engine2(), fake());
    let k = key_of(&f.bind(&e, None));
    assert!(f.fire(&e, &k, "enter", &[("stateMachine", "plan")]).ok);
    let r = f.fire(&e, &k, "written", &[("planId", "PLAN-3")]);
    assert!(r.ok, "{}", r.text);
    assert!(f.fire(&e, &k, "revise", &[]).ok);
    let r = f.fire(&e, &k, "written", &[("planId", "PLAN-3")]);
    assert!(r.ok, "{}", r.text);
    assert_eq!(r.location.r#ref.as_deref(), Some("PLAN-3"));
    assert!(f.fire(&e, &k, "revise", &[]).ok);
    let r = f.fire(&e, &k, "written", &[("planId", "PLAN-4")]);
    assert!(
        !r.ok && r.text.contains("already has its ref PLAN-3"),
        "{}",
        r.text
    );
}

// Only the chosen transition's setRef is checked (PLAN-003 F10).
// @zen-test: INST-3_AC-1
#[test]
fn set_ref_is_checked_only_on_the_chosen_branch() {
    let (e, mut f) = (engine2(), fake());
    let k = key_of(&f.bind(&e, None));
    f.fire(&e, &k, "enter", &[("stateMachine", "plan")]);
    // First visit: the setRef branch's guard fails, the plain one is taken.
    let r = f.fire(&e, &k, "go", &[]);
    assert!(r.ok, "{}", r.text);
    assert_eq!(r.location.state.as_deref(), Some("GRILL"));
    f.fire(&e, &k, "revise", &[]);
    // Second visit: the setRef branch is chosen and needs its param.
    let r = f.fire(&e, &k, "go", &[]);
    assert!(
        !r.ok && r.text.contains("go needs param planId"),
        "{}",
        r.text
    );
}

// A session whose instance was taken over enters from idle on its next
// `enter`, rather than being rejected once (PLAN-003 F12).
// @zen-test: INST-7_AC-1
#[test]
fn after_a_takeover_enter_goes_on_from_idle() {
    let (e, mut f) = (engine2(), fake());
    let a = key_of(&f.bind(&e, Some("a")));
    let b = key_of(&f.bind(&e, Some("b")));
    let gh1 = [("stateMachine", "dev"), ("issueId", "GH-1")];
    f.fire(&e, &a, "enter", &gh1);
    f.fire(&e, &b, "enter", &gh1);
    assert!(f.view(&e, &a).text.contains("moved to session"));
    let r = f.fire(&e, &a, "enter", &[("stateMachine", "help")]);
    assert!(r.ok, "{}", r.text);
    assert_eq!(r.location.machine.as_deref(), Some("help"));
}

// A rejected keyless `enter` saved no session, so it shows no key
// (PLAN-003 F13, D3-6).
// @zen-test: TURN-3_AC-1
#[test]
fn a_rejected_keyless_enter_shows_no_key() {
    let (e, mut f) = (engine2(), fake());
    let params = vec![("stateMachine".to_string(), "nope".to_string())];
    let r = f.with(|h| e.fire(h, None, "enter", &params, &Bind::default()).unwrap());
    assert!(!r.ok);
    assert!(r.session.is_empty());
    assert!(r.text.contains("no session · idle"), "{}", r.text);
    assert!(r.text.contains("smllm({ event, params })"), "{}", r.text);
    assert!(!r.text.contains("sm-"), "{}", r.text);
    // Accepted, it has a key.
    let params = vec![("stateMachine".to_string(), "help".to_string())];
    let r = f.with(|h| e.fire(h, None, "enter", &params, &Bind::default()).unwrap());
    assert!(r.ok && r.session.starts_with("sm-"), "{}", r.text);
}

// Exit prompts are shown on park and suspend too (PLAN-003 F14).
// @zen-test: ACT-2_AC-1
#[test]
fn exit_prompts_show_on_park() {
    let (e, mut f) = (engine2(), fake());
    let k = key_of(&f.bind(&e, None));
    f.fire(&e, &k, "enter", &[("stateMachine", "help")]);
    let r = f.fire(&e, &k, "park", &[]);
    assert!(
        r.ok && r.text.contains("Leaving ASK: note where you were."),
        "{}",
        r.text
    );
}

// One value per param at `enter` (PLAN-003 F15).
// @zen-test: IDLE-3_AC-1
#[test]
fn enter_refuses_a_repeated_param() {
    let (e, mut f) = (engine2(), fake());
    let k = key_of(&f.bind(&e, None));
    let r = f.fire(
        &e,
        &k,
        "enter",
        &[
            ("stateMachine", "dev"),
            ("issueId", "GH-1"),
            ("issueId", "$(rm -rf ~)"),
        ],
    );
    assert!(
        !r.ok && r.text.contains("param issueId more than once"),
        "{}",
        r.text
    );
}

// `resume` while the machine is unconfigured keeps the instance suspended
// (PLAN-003 F16).
// @zen-test: IDLE-2_AC-1
#[test]
fn resume_with_the_machine_missing_keeps_the_suspended_instance() {
    let (e, mut f) = (engine2(), fake());
    let k = key_of(&f.bind(&e, None));
    f.fire(
        &e,
        &k,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-1")],
    );
    assert!(f.fire(&e, &k, "unmatched", &[]).ok);
    let broken = Engine::new(Config {
        machines: vec![help()],
        idle: vec![],
    });
    let r = f.fire(&broken, &k, "resume", &[]);
    assert!(
        !r.ok && r.text.contains("fix the config, then resume"),
        "{}",
        r.text
    );
    let r = f.fire(&e, &k, "resume", &[]);
    assert!(r.ok, "{}", r.text);
    assert_eq!(r.location.state.as_deref(), Some("TRIAGE"));
}

// A transition back into the fallback state keeps `resume` (PLAN-003 F17).
// @zen-test: IDLE-2_AC-1
#[test]
fn a_self_transition_in_the_fallback_keeps_resume() {
    let (e, mut f) = (engine2(), fake());
    let k = key_of(&f.bind(&e, None));
    f.fire(&e, &k, "enter", &[("stateMachine", "help")]);
    let r = f.fire(&e, &k, "unmatched", &[]);
    assert_eq!(r.location.state.as_deref(), Some("ASIDE"), "{}", r.text);
    assert!(f.fire(&e, &k, "note", &[]).ok);
    let r = f.fire(&e, &k, "resume", &[]);
    assert!(r.ok, "{}", r.text);
    assert_eq!(r.location.state.as_deref(), Some("ASK"));
}

// The error says "fix the config, or park": park and enter work, letting
// go of the instance without touching it (PLAN-003 F7).
// @zen-test: INST-7_AC-1
#[test]
fn park_or_enter_leaves_an_unconfigured_machine() {
    let (e, mut f) = (engine(), fake());
    let k = key_of(&f.bind(&e, None));
    f.fire(
        &e,
        &k,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-1")],
    );
    let broken = Engine::new(Config {
        machines: vec![helpdesk()],
        idle: vec![],
    });
    let r = f.fire(&broken, &k, "park", &[]);
    assert!(r.ok && r.text.contains("· idle"), "{}", r.text);
    assert!(r.text.contains("Let go of instance"), "{}", r.text);
    // Back with the config fixed, the instance is where it was.
    let r = f.fire(
        &e,
        &k,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-1")],
    );
    assert!(r.ok, "{}", r.text);
    assert_eq!(r.location.state.as_deref(), Some("TRIAGE"));
    // enter goes straight on from idle.
    let r = f.fire(&broken, &k, "enter", &[("stateMachine", "help")]);
    assert!(r.ok, "{}", r.text);
    assert_eq!(r.location.machine.as_deref(), Some("help"));
}
