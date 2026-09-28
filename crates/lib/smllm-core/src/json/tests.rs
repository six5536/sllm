//! The json feature writes and reads the serde feature's JSON, byte for byte.

use super::*;

fn same<T>(v: &T)
where
    T: ToJson
        + FromJson
        + serde::Serialize
        + serde::de::DeserializeOwned
        + PartialEq
        + core::fmt::Debug,
{
    let ours = smllm_json::to_string(v);
    let theirs = serde_json::to_string(v).unwrap();
    assert_eq!(ours, theirs);
    let back: T = smllm_json::from_str(&theirs).unwrap();
    assert_eq!(&back, v);
}

fn written_same<T: ToJson + serde::Serialize>(v: &T) {
    assert_eq!(smllm_json::to_string(v), serde_json::to_string(v).unwrap());
}

#[test]
fn dev_json_is_the_same_both_ways() {
    let text = include_str!("../../../../../packages/smllm-wasm/test/dev.json");
    let c: Config = serde_json::from_str(text).unwrap();
    same(&c);
    assert_eq!(smllm_json::to_string(&c), text.trim_end());
}

fn key() -> InstanceKey {
    InstanceKey {
        machine: "m".into(),
        id: "i\u{e9}\"\n\u{1}\u{7f}\u{1F600}".into(),
    }
}

fn session() -> Session {
    Session {
        key: "sm-1".into(),
        harness: "h".into(),
        host_session: Some("x".into()),
        cwd: "/w".into(),
        configs: vec!["a".into()],
        holding: Some(key()),
        interrupted: None,
        yielded: true,
        blocked: false,
        created: u64::MAX,
        last_active: 2,
    }
}

fn instance() -> Instance {
    Instance {
        id: "i".into(),
        machine: "m".into(),
        r#ref: Some("GH-1".into()),
        state: "S".into(),
        status: Status::Paused,
        holder: None,
        version: 3,
        visits: [("S", 2u32)].into_iter().collect(),
        resume_state: None,
        created: 1,
        updated: 2,
    }
}

#[test]
fn records_are_the_same_both_ways() {
    let h = HistoryEntry {
        at: 1,
        session: "s".into(),
        event: "e".into(),
        from: None,
        to: Some("T".into()),
        params: [("p", String::from("v"))].into_iter().collect(),
        trace: vec!["t".into()],
    };
    let parts = |history: SmallMap<Vec<HistoryEntry>>| {
        MemoryStore::from_parts(
            [("sm-1", session())].into_iter().collect(),
            [("h/x", String::from("sm-1"))].into_iter().collect(),
            [("m/i", instance())].into_iter().collect(),
            history,
        )
    };
    same(&parts(SmallMap::new()));
    same(&parts([("m/i", vec![h.clone()])].into_iter().collect()));
    same(&session());
    same(&instance());
    same(&h);
    same(&key());
    for s in [
        Status::Active,
        Status::Interrupted,
        Status::Paused,
        Status::Completed,
    ] {
        same(&s);
    }
    // Absent default fields read as serde reads them.
    let min = r#"{"key":"k","harness":"h","cwd":"c","created":1,"lastActive":2}"#;
    let a: Session = smllm_json::from_str(min).unwrap();
    let b: Session = serde_json::from_str(min).unwrap();
    assert_eq!(a, b);
}

#[test]
fn engine_answers_are_written_the_same() {
    let location = Location {
        machine: Some("m".into()),
        state: None,
        instance: Some("i".into()),
        r#ref: None,
    };
    written_same(&Reply {
        ok: true,
        session: "sm-1".into(),
        location,
        text: "<smllm>\n\"x\"</smllm>".into(),
    });
    let inst = InstanceStatus {
        machine: "m".into(),
        kind: "issue".into(),
        id: "i".into(),
        r#ref: Some("GH-1".into()),
        label: "GH-1".into(),
        status: "active".into(),
    };
    written_same(&SessionStatus {
        session: "sm-1".into(),
        idle: false,
        machine: Some("m".into()),
        state: Some("S".into()),
        visit: Some(2),
        yielded: false,
        instance: Some(inst),
        interrupted: None,
        paused: 3,
    });
}

#[test]
fn every_guard_action_and_prompt_is_the_same_both_ways() {
    let params: SmallMap<Value> = [
        ("run", Value::List(vec!["cargo".into(), "test".into()])),
        ("cwd", Value::Str("sub".into())),
        ("timeoutSecs", Value::Int(i64::MAX)),
        ("low", Value::Int(i64::MIN)),
        ("quiet", Value::Bool(false)),
    ]
    .into_iter()
    .collect();
    same(&GuardDef::Visits {
        state: "S".into(),
        at_least: 3,
    });
    same(&GuardDef::Host {
        kind: "command".into(),
        params: params.clone(),
    });
    same(&ActionDef::SetRef);
    same(&ActionDef::Host {
        kind: "command".into(),
        params,
    });
    for p in [
        Prompt::Text("t".into()),
        Prompt::File("/f.md".into()),
        Prompt::DefaultFile("/enter-S.md".into()),
    ] {
        same(&ActionDef::Prompt(p));
    }
    same(&Position::Before);
    same(&Position::After);
}

#[test]
fn what_serde_rejects_smllm_json_rejects() {
    fn both_reject<T: FromJson + serde::de::DeserializeOwned>(json: &str) {
        assert!(
            serde_json::from_str::<T>(json).is_err(),
            "serde took {json}"
        );
        assert!(
            smllm_json::from_str::<T>(json).is_err(),
            "smllm-json took {json}"
        );
    }
    both_reject::<Value>("9223372036854775808");
    both_reject::<Value>("1.5");
    both_reject::<Value>("null");
    both_reject::<Value>("[1]");
    both_reject::<ActionDef>(r#""setref""#);
    both_reject::<ActionDef>(r#"{"shell":{}}"#);
    both_reject::<ActionDef>(r#"{"prompt":{"text":"a","file":"b"}}"#);
    both_reject::<Prompt>(r#"{"html":"a"}"#);
    both_reject::<Prompt>("{}");
    both_reject::<GuardDef>(r#"{"visits":{"state":"S"}}"#);
    both_reject::<GuardDef>(
        r#"{"visits":{"state":"S","at_least":1},"host":{"kind":"k","params":{}}}"#,
    );
    both_reject::<Position>(r#""middle""#);
    both_reject::<Status>(r#""Active""#);
    both_reject::<Session>(r#"{"key":"k"}"#);
    both_reject::<Session>(
        r#"{"key":"k","key":"l","harness":"h","cwd":"c","created":1,"lastActive":2}"#,
    );
    both_reject::<Instance>(
        r#"{"id":"i","machine":"m","state":"S","status":"active","version":-1,"created":1,"updated":2}"#,
    );
    both_reject::<Config>(r#"{"machines":[]} x"#);
    both_reject::<Config>(r#"{"machines":[],}"#);
    both_reject::<Config>(r#"{"machines":[],"zzz":}"#);
    both_reject::<Config>(r#"{"machines":[],"zzz":["#);
}

#[test]
fn errors_name_the_field() {
    let err = |t: &str| smllm_json::from_str::<Config>(t).unwrap_err().to_string();
    assert_eq!(
        err(r#"{"machines":[{"id":1}]}"#),
        "machines[0].id: expected a string, found an integer"
    );
    assert_eq!(
        err(r#"{"machines":[],"idle":[{"prompt":{"text":2}}]}"#),
        "idle[0].prompt.text: expected a string, found an integer"
    );
    assert_eq!(err(r#"{"idle":[]}"#), "missing field `machines`");
    assert_eq!(err("{"), "expected a key string at byte 1");
}
