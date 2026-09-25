//! A fake host and a small machine builder for the engine tests.
#![allow(dead_code, clippy::type_complexity)]

use std::cell::RefCell;
use std::collections::HashMap;

use smllm_core::host::{
    Action, Call, Clock, Guard, Host, Ids, InstructionSource, Matcher, MemoryStore, Outcome,
};
use smllm_core::model::{
    ActionDef, Config, EventDef, GuardDef, InstanceSpec, Machine, On, ParamSpec, Position, Prompt,
    SharedAction, State, Transition, Value,
};
use smllm_core::{Bind, Engine, Reply, SmallMap, Stop};

/// Scripted guards/actions, recorded calls, fixed time, counting ids.
#[derive(Default)]
pub struct Fake {
    pub store: MemoryStore,
    /// `run` string → guard result; missing = true.
    pub guard_results: HashMap<String, bool>,
    /// `run` strings of actions that fail.
    pub failing_actions: Vec<String>,
    /// (kind, run, env) of every action run.
    pub ran: RefCell<Vec<(String, String, Vec<(String, String)>)>>,
    pub files: HashMap<String, String>,
    pub now: u64,
    pub next: u64,
    pub no_host_kinds: bool,
}

struct G<'a>(&'a Fake);
struct A<'a>(&'a Fake);
struct S<'a>(&'a Fake);
struct C(u64);
struct I<'a>(&'a mut u64);

fn run_of(call: &Call<'_>) -> String {
    match call.params.get("run") {
        Some(Value::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

impl Guard for G<'_> {
    fn supports(&self, kind: &str) -> bool {
        !self.0.no_host_kinds && kind == "command"
    }
    fn check(&mut self, call: &Call<'_>) -> Outcome {
        let ok = *self.0.guard_results.get(&run_of(call)).unwrap_or(&true);
        Outcome {
            ok,
            detail: if ok { String::new() } else { "exit 1".into() },
        }
    }
}

impl Action for A<'_> {
    fn supports(&self, kind: &str) -> bool {
        !self.0.no_host_kinds && kind == "command"
    }
    fn run(&mut self, call: &Call<'_>) -> Outcome {
        let run = run_of(call);
        self.0
            .ran
            .borrow_mut()
            .push((call.kind.to_string(), run.clone(), call.env.to_vec()));
        let ok = !self.0.failing_actions.contains(&run);
        Outcome {
            ok,
            detail: if ok {
                String::new()
            } else {
                "exited 128: boom".into()
            },
        }
    }
}

impl InstructionSource for S<'_> {
    fn read(&self, file: &str) -> Result<Option<String>, String> {
        if file == "unreadable.md" {
            return Err("permission denied".into());
        }
        Ok(self.0.files.get(file).cloned())
    }
}

/// `^GH-\d+$` and `^[a-z]+$` stand-ins; anything else matches.
impl Matcher for S<'_> {
    fn is_match(&self, pattern: &str, value: &str) -> Result<bool, String> {
        match pattern {
            "^GH-\\d+$" => Ok(value
                .strip_prefix("GH-")
                .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))),
            "(" => Err("unclosed group".into()),
            _ => Ok(true),
        }
    }
}

impl Clock for C {
    fn now_ms(&self) -> u64 {
        self.0
    }
}

impl Ids for I<'_> {
    fn random(&mut self) -> u64 {
        *self.0 += 1;
        self.0.wrapping_mul(0x9E37_79B9_7F4A_7C15)
    }
}

impl Fake {
    pub fn new() -> Self {
        Self {
            now: 1_790_332_320_000,
            ..Self::default()
        }
    }

    /// Run `f` with a [`Host`] over this fake.
    pub fn with<R>(&mut self, f: impl FnOnce(&mut Host<'_>) -> R) -> R {
        let mut store = std::mem::take(&mut self.store);
        let mut next = self.next;
        let r = {
            let this: &Fake = self;
            let mut g = G(this);
            let mut a = A(this);
            let s = S(this);
            let c = C(this.now);
            let mut i = I(&mut next);
            let mut host = Host {
                store: &mut store,
                guards: &mut g,
                actions: &mut a,
                source: &s,
                matcher: &s,
                clock: &c,
                ids: &mut i,
            };
            f(&mut host)
        };
        self.store = store;
        self.next = next;
        r
    }

    pub fn bind(&mut self, engine: &Engine, host_session: Option<&str>) -> Reply {
        self.with(|h| {
            engine
                .bind(
                    h,
                    &Bind {
                        harness: "claude",
                        host_session,
                        cwd: "/work",
                        configs: &[],
                    },
                )
                .unwrap()
        })
    }

    pub fn fire(
        &mut self,
        engine: &Engine,
        key: &str,
        event: &str,
        params: &[(&str, &str)],
    ) -> Reply {
        let params: Vec<(String, String)> = params
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        self.with(|h| {
            engine
                .fire(h, Some(key), event, &params, &Bind::default())
                .unwrap()
        })
    }

    pub fn view(&mut self, engine: &Engine, key: &str) -> Reply {
        self.with(|h| engine.view(h, key).unwrap())
    }

    pub fn stop(&mut self, engine: &Engine, key: &str, active: bool) -> Stop {
        self.with(|h| engine.stop(h, key, active).unwrap())
    }
}

// ---- builder -------------------------------------------------------------

pub fn text(t: &str) -> ActionDef {
    ActionDef::Prompt(Prompt::Text(t.into()))
}

pub fn file(f: &str) -> ActionDef {
    ActionDef::Prompt(Prompt::File(f.into()))
}

pub fn cmd(run: &str) -> ActionDef {
    ActionDef::Host {
        kind: "command".into(),
        params: params_run(run),
    }
}

pub fn params_run(run: &str) -> SmallMap<Value> {
    [("run", Value::Str(run.into()))].into_iter().collect()
}

pub fn guard_cmd(run: &str) -> GuardDef {
    GuardDef::Host {
        kind: "command".into(),
        params: params_run(run),
    }
}

pub fn to(target: &str) -> Transition {
    Transition {
        target: Some(target.into()),
        ..Transition::default()
    }
}

pub fn on(event: &str, transitions: Vec<Transition>) -> On {
    On {
        event: event.into(),
        transitions,
    }
}

pub fn state(name: &str) -> State {
    State {
        name: name.into(),
        ..State::default()
    }
}

pub fn param(name: &str, required: bool) -> ParamSpec {
    ParamSpec {
        name: name.into(),
        required,
        ..ParamSpec::default()
    }
}

/// The plan's `dev` example (§3), lowered by hand.
pub fn dev() -> Machine {
    let mut events = SmallMap::new();
    events.insert(
        "reject",
        EventDef {
            description: Some("Select when the work needs changes.".into()),
            params: vec![
                ParamSpec {
                    description: Some("What is missing.".into()),
                    ..param("reason", true)
                },
                ParamSpec {
                    description: Some("How much rework.".into()),
                    enum_values: vec!["minor".into(), "major".into()],
                    ..param("severity", false)
                },
            ],
        },
    );
    events.insert(
        "submit",
        EventDef {
            description: None,
            params: vec![ParamSpec {
                description: Some("One-line commit message for the change.".into()),
                ..param("summary", true)
            }],
        },
    );
    events.insert(
        "issueCreated",
        EventDef {
            description: Some("Select once the issue exists.".into()),
            params: vec![ParamSpec {
                description: Some("The GitHub issue id, e.g. GH-123.".into()),
                pattern: Some("^GH-\\d+$".into()),
                ..param("issueId", true)
            }],
        },
    );

    let mut triage = state("TRIAGE");
    triage.entry_point = true;
    triage.entry = vec![file("triage.md")];
    triage.on = vec![
        on(
            "issueCreated",
            vec![Transition {
                actions: vec![ActionDef::SetRef],
                ..to("WORK")
            }],
        ),
        on("accept", vec![to("CHECK")]),
    ];

    let mut check = state("CHECK");
    check.always = vec![
        Transition {
            guard: Some(guard_cmd("gh issue view \"$SMLLM_REF\"")),
            ..to("WORK")
        },
        to("TRIAGE"),
    ];

    let mut work = state("WORK");
    work.entry = vec![cmd("git switch issue"), file("work.md")];
    work.on = vec![on(
        "submit",
        vec![
            Transition {
                guard: Some(guard_cmd("cargo test --quiet")),
                actions: vec![cmd("git commit -am \"$SMLLM_PARAM_SUMMARY\"")],
                ..to("REVIEW")
            },
            Transition {
                guard: Some(GuardDef::Visits {
                    state: "WORK".into(),
                    at_least: 3,
                }),
                ..to("ESCALATE")
            },
            Transition {
                reenter: true,
                ..to("WORK")
            },
        ],
    )];

    let mut review = state("REVIEW");
    review.entry_point = true;
    review.on = vec![
        on(
            "approve",
            vec![Transition {
                actions: vec![cmd("gh pr merge --auto"), text("Confirm the PR merged.")],
                ..to("DONE")
            }],
        ),
        on(
            "reject",
            vec![Transition {
                description: Some("Select when a review checklist item fails.".into()),
                ..to("WORK")
            }],
        ),
    ];
    let mut pd = SmallMap::new();
    pd.insert("reason", "Which checklist item failed.".to_string());
    review.param_descriptions.insert("reject", pd);

    let mut escalate = state("ESCALATE");
    escalate.on = vec![on("resolved", vec![to("WORK")])];

    let mut done = state("DONE");
    done.is_final = true;
    done.entry = vec![text("All done.")];

    Machine {
        id: "dev".into(),
        description: Some("Fix an issue end to end.".into()),
        initial: "TRIAGE".into(),
        instance: InstanceSpec {
            noun: "issue".into(),
            ref_param: "issueId".into(),
            ref_description: Some("The GitHub issue id, e.g. GH-123.".into()),
            ref_pattern: Some("^GH-\\d+$".into()),
        },
        events,
        shared: vec![SharedAction {
            states: vec!["WORK".into(), "REVIEW".into()],
            position: Position::Before,
            entry: vec![text("Work on one issue at a time.")],
            exit: vec![],
        }],
        states: vec![triage, check, work, review, escalate, done],
    }
}

/// A two-state machine with a fallback state.
pub fn helpdesk() -> Machine {
    let mut ask = state("ASK");
    ask.entry_point = true;
    ask.entry = vec![text("Ask.")];
    ask.on = vec![on("answered", vec![to("CLOSED")])];
    let mut aside = state("ASIDE");
    aside.fallback = true;
    aside.entry = vec![text("Handle the aside.")];
    aside.on = vec![on("handled", vec![to("ASK")])];
    let mut closed = state("CLOSED");
    closed.is_final = true;
    Machine {
        id: "help".into(),
        description: None,
        initial: "ASK".into(),
        instance: InstanceSpec::default(),
        events: SmallMap::new(),
        shared: vec![],
        states: vec![ask, aside, closed],
    }
}

pub fn engine() -> Engine {
    Engine::new(Config {
        machines: vec![dev(), helpdesk()],
        idle: vec![text("Pick work from the list.")],
    })
}

pub fn fake() -> Fake {
    let mut f = Fake::new();
    f.files.insert(
        "triage.md".into(),
        "Decide whether the issue is real.".into(),
    );
    f.files
        .insert("work.md".into(), "Fix it. Add tests.".into());
    f
}
