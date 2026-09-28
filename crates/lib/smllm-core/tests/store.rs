//! `MemoryStore`'s indexes answer as a plain scan would (ENG_P-5).

use std::collections::BTreeMap;

use proptest::prelude::*;
use smllm_core::host::{MemoryStore, Store, newest_first};
use smllm_core::record::{Instance, Status};

const MACHINES: [&str; 2] = ["a", "b"];
const STATUSES: [Status; 4] = [
    Status::Active,
    Status::Interrupted,
    Status::Paused,
    Status::Completed,
];
const REFS: [Option<&str>; 4] = [None, Some("R0"), Some("R1"), Some("R2")];

/// One write: machine, instance, status, ref.
fn step() -> impl Strategy<Value = (usize, usize, usize, usize)> {
    (
        0..MACHINES.len(),
        0..5usize,
        0..STATUSES.len(),
        0..REFS.len(),
    )
}

type Model = BTreeMap<(String, String), Instance>;

fn check(store: &mut MemoryStore, model: &Model) -> Result<(), TestCaseError> {
    for m in MACHINES {
        let of = |pred: &dyn Fn(&Instance) -> bool| -> Vec<Instance> {
            model
                .values()
                .filter(|i| i.machine == m && pred(i))
                .cloned()
                .collect()
        };
        for s in STATUSES {
            let want = of(&|i| i.status == s);
            let mut got = store.instances_with(m, s).unwrap();
            got.sort_by(|a, b| a.id.cmp(&b.id));
            prop_assert_eq!(&got, &want);
            prop_assert_eq!(store.count(m, s).unwrap(), want.len());
            let mut newest = want.clone();
            newest_first(&mut newest);
            newest.truncate(3);
            prop_assert_eq!(store.recent(m, s, 3).unwrap(), newest);
        }
        for r in REFS.iter().flatten() {
            let want = of(&|i| i.r#ref.as_deref() == Some(r)).into_iter().next();
            prop_assert_eq!(store.instance_by_ref(m, r).unwrap(), want);
        }
    }
    // Built again from its saved parts, the store has the same indexes.
    let rebuilt = MemoryStore::from_parts(
        store.sessions().clone(),
        store.bindings().clone(),
        store.all_instances().clone(),
        store.history().clone(),
    );
    prop_assert_eq!(&rebuilt, &*store);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    // @zen-test: ENG_P-5
    #[test]
    fn indexes_match_scans(steps in proptest::collection::vec(step(), 0..40)) {
        let mut store = MemoryStore::default();
        let mut model = Model::new();
        for (n, (m, id, s, r)) in steps.into_iter().enumerate() {
            let key = (MACHINES[m].to_string(), format!("i-{id}"));
            let mut i = model.get(&key).cloned().unwrap_or(Instance {
                id: key.1.clone(),
                machine: key.0.clone(),
                r#ref: None,
                state: "S".into(),
                status: Status::Active,
                holder: None,
                version: 0,
                visits: Default::default(),
                resume_state: None,
                created: 0,
                updated: 0,
            });
            i.version += 1;
            i.status = STATUSES[s];
            i.r#ref = REFS[r].map(str::to_string);
            // Few distinct times, so ties happen and labels decide.
            i.updated = (n / 3) as u64;
            if store.put_instance(&i).is_ok() {
                model.insert(key, i);
            }
            check(&mut store, &model)?;
        }
    }
}
