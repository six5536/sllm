//! Engine properties over random event sequences (TEST-1, NFR-3).
#![allow(clippy::type_complexity)]

mod support;

use proptest::prelude::*;
use smllm_core::host::Store;
use support::*;

const EVENTS: &[&str] = &[
    "enter",
    "resume",
    "park",
    "unmatched",
    "yield",
    "issueCreated",
    "accept",
    "submit",
    "approve",
    "reject",
    "resolved",
    "answered",
    "handled",
    "bogus",
];
const PARAMS: &[(&str, &[&str])] = &[
    ("stateMachine", &["dev", "help", "nope"]),
    ("issueId", &["GH-1", "GH-2", "bad"]),
    ("ref", &["r1"]),
    ("state", &["TRIAGE", "REVIEW", "WORK", "ASK"]),
    ("summary", &["s"]),
    ("reason", &["r"]),
    ("severity", &["minor", "huge"]),
];

fn step() -> impl Strategy<Value = (usize, Vec<(usize, usize)>, bool)> {
    (
        0..EVENTS.len(),
        proptest::collection::vec((0..PARAMS.len(), 0..4usize), 0..3),
        any::<bool>(),
    )
}

fn params(ps: &[(usize, usize)]) -> Vec<(&'static str, &'static str)> {
    let mut out: Vec<(&str, &str)> = Vec::new();
    for &(p, v) in ps {
        let (name, values) = PARAMS[p];
        if !out.iter().any(|(n, _)| *n == name) {
            out.push((name, values[v % values.len()]));
        }
    }
    out
}

/// Run a sequence; return every reply's text plus the final store.
fn run(
    steps: &[(usize, Vec<(usize, usize)>, bool)],
) -> (Vec<String>, smllm_core::host::MemoryStore) {
    let e = engine();
    let mut f = fake();
    let k = f.bind(&e, None).session;
    let mut texts = Vec::new();
    for (ev, ps, guard) in steps {
        f.guard_results.insert("cargo test --quiet".into(), *guard);
        f.guard_results
            .insert("gh issue view \"$SMLLM_REF\"".into(), !*guard);
        texts.push(f.fire(&e, &k, EVENTS[*ev], &params(ps)).text);
    }
    (texts, f.store)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    // @zen-test: ENG_P-1
    #[test]
    fn deterministic(steps in proptest::collection::vec(step(), 0..25)) {
        prop_assert_eq!(run(&steps), run(&steps));
    }

    // @zen-test: ENG_P-2
    // @zen-test: ENG_P-3
    // @zen-test: ENG_P-4
    #[test]
    fn invariants_hold(steps in proptest::collection::vec(step(), 0..25)) {
        let e = engine();
        let mut f = fake();
        let k = f.bind(&e, None).session;
        for (ev, ps, guard) in &steps {
            f.guard_results.insert("cargo test --quiet".into(), *guard);
            let before = f.store.clone();
            let r = f.fire(&e, &k, EVENTS[*ev], &params(ps));
            if !r.ok {
                // An invalid call never changes state.
                prop_assert_eq!(&f.store, &before, "{}", r.text);
            }
            // Never rest in an always state.
            if let Some(s) = &r.location.state {
                let m = e.config().machine(r.location.machine.as_deref().unwrap()).unwrap();
                prop_assert!(m.state(s).is_some_and(|s| s.always.is_empty()), "rested in {}", s);
            }
            // Visits never go down.
            for m in ["dev", "help"] {
                let now = f.with(|h| h.store.instances(m).unwrap());
                let mut old = before.clone();
                for i in now {
                    if let Some(prev) = old.instance(m, &i.id).unwrap() {
                        for (state, n) in prev.visits.iter() {
                            prop_assert!(i.visits(state) >= *n);
                        }
                        prop_assert!(i.version >= prev.version);
                    }
                }
            }
        }
    }
}
