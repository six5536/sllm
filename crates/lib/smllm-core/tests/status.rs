//! `Engine::status`: where a session is, as data (STL).

mod support;

use proptest::prelude::*;
use smllm_core::{Error, InstanceStatus, SessionStatus};
use support::*;

fn status(f: &mut Fake, e: &smllm_core::Engine, k: &str) -> SessionStatus {
    f.with(|h| e.status(h, k).unwrap())
}

fn issue(id: &str, r: Option<&str>, status: &str) -> InstanceStatus {
    InstanceStatus {
        machine: "dev".into(),
        kind: "issue".into(),
        id: id.into(),
        r#ref: r.map(Into::into),
        label: r.unwrap_or(id).into(),
        status: status.into(),
    }
}

// @zen-test: STL-5_AC-1
// @zen-test: STL-5_AC-2
// @zen-test: STL-8_AC-1
#[test]
fn status_follows_the_session_through_a_machine_and_idle() {
    let (e, mut f) = (engine(), fake());
    let k = f.bind(&e, None).session;
    assert_eq!(
        status(&mut f, &e, &k),
        SessionStatus {
            session: k.clone(),
            idle: true,
            machine: None,
            state: None,
            visit: None,
            yielded: false,
            instance: None,
            suspended: None,
            parked: 0,
        }
    );

    f.fire(
        &e,
        &k,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-6")],
    );
    let id = f.with(|h| h.store.instances("dev").unwrap())[0].id.clone();
    let s = status(&mut f, &e, &k);
    assert!(!s.idle);
    assert_eq!(s.machine.as_deref(), Some("dev"));
    assert_eq!(s.state.as_deref(), Some("TRIAGE"));
    assert_eq!(s.visit, Some(1));
    assert_eq!(s.instance, Some(issue(&id, Some("GH-6"), "active")));

    f.fire(&e, &k, "yield", &[]);
    assert!(status(&mut f, &e, &k).yielded);

    // Detour: the held instance becomes the suspended one.
    f.fire(&e, &k, "unmatched", &[]);
    let s = status(&mut f, &e, &k);
    assert!(s.idle && s.instance.is_none());
    assert_eq!(s.suspended, Some(issue(&id, Some("GH-6"), "suspended")));

    // Parked instances are counted.
    f.fire(&e, &k, "resume", &[]);
    f.fire(&e, &k, "park", &[]);
    let s = status(&mut f, &e, &k);
    assert!(s.idle && s.suspended.is_none());
    assert_eq!(s.parked, 1);
}

#[test]
fn a_moved_instance_reads_as_idle_and_unknown_sessions_are_errors() {
    let (e, mut f) = (engine(), fake());
    let a = f.bind(&e, Some("a")).session;
    let b = f.bind(&e, Some("b")).session;
    f.fire(
        &e,
        &a,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-1")],
    );
    f.fire(
        &e,
        &b,
        "enter",
        &[("stateMachine", "dev"), ("issueId", "GH-1")],
    );
    assert!(status(&mut f, &e, &a).idle);
    assert_eq!(status(&mut f, &e, &b).state.as_deref(), Some("TRIAGE"));
    assert_eq!(
        f.with(|h| e.status(h, "sm-nope")),
        Err(Error::UnknownSession("sm-nope".into()))
    );
}

#[test]
fn an_unconfigured_machine_reads_as_idle() {
    let (e, mut f) = (engine(), fake());
    let k = f.bind(&e, None).session;
    f.fire(&e, &k, "enter", &[("stateMachine", "help")]);
    let only_dev = smllm_core::Engine::new(smllm_core::model::Config {
        machines: vec![dev()],
        idle: vec![],
    });
    let s = status(&mut f, &only_dev, &k);
    assert!(s.idle && s.machine.is_none());
}

const EVENTS: &[(&str, &[(&str, &str)])] = &[
    ("enter", &[("stateMachine", "dev"), ("issueId", "GH-1")]),
    ("enter", &[("stateMachine", "help")]),
    ("issueCreated", &[("issueId", "GH-2")]),
    ("accept", &[]),
    ("unmatched", &[]),
    ("resume", &[]),
    ("park", &[]),
    ("yield", &[]),
    ("answered", &[]),
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    // @zen-test: STL_P-1
    // @zen-test: STL_P-2
    // @zen-test: STL-3_AC-1
    #[test]
    fn status_reads_only_and_agrees_with_the_view(
        steps in proptest::collection::vec(0..EVENTS.len(), 0..20)
    ) {
        let (e, mut f) = (engine(), fake());
        let k = f.bind(&e, None).session;
        for i in steps {
            let (ev, ps) = EVENTS[i];
            f.fire(&e, &k, ev, ps);
            let (before, ran) = (f.store.clone(), f.ran.borrow().len());
            let s = status(&mut f, &e, &k);
            prop_assert_eq!(&f.store, &before);
            prop_assert_eq!(f.ran.borrow().len(), ran);
            let view = f.view(&e, &k);
            prop_assert_eq!(s.idle, view.location.machine.is_none(), "{}", view.text);
            prop_assert_eq!(s.state, view.location.state);
        }
    }
}
