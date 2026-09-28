//! miniserde impls writing and reading the serde feature's JSON, byte for
//! byte (PLAN-007 D7-1), for what the miniserde derive cannot say:
//! data-carrying enums, the untagged `Value`, `serde(default)` fields and a
//! skipped empty field. The derive also rejects any `serde(...)` field
//! attribute but `rename`, so a type with `serde(default)` fields cannot
//! derive while both features are on. The tests prove the two agree.
// @zen-component: ENG-Model

use alloc::borrow::Cow;
use miniserde::de::{self, Deserialize, Visitor};
use miniserde::ser::{self, Fragment, Serialize};
use miniserde::{Error, Result, make_place};

use crate::host::MemoryStore;
use crate::model::{ActionDef, Config, GuardDef, Prompt, Value};
use crate::prelude::*;
use crate::record::{HistoryEntry, Instance, Session};
use crate::utils::SmallMap;

/// A field's start value: `req` must be present (an `Option` may be absent),
/// `default`/`skip` start at `Default::default()`.
macro_rules! init {
    (req) => {
        Deserialize::default()
    };
    ($m:ident) => {
        Some(Default::default())
    };
}

/// Whether a field is left out of the JSON: `skip` while empty.
macro_rules! skipped {
    (skip, $v:expr) => {
        $v.is_empty()
    };
    ($m:ident, $v:expr) => {
        false
    };
}

/// Serialize + Deserialize for a struct as a JSON object of the given names.
macro_rules! object {
    ($ty:ident { $($f:ident : $t:ty = $name:literal $mode:ident),* $(,)? }) => {
        impl Serialize for $ty {
            fn begin(&self) -> Fragment<'_> {
                struct S<'a> {
                    d: &'a $ty,
                    i: usize,
                }
                impl ser::Map for S<'_> {
                    fn next(&mut self) -> Option<(Cow<'_, str>, &dyn Serialize)> {
                        loop {
                            let i = self.i;
                            self.i += 1;
                            let mut n = 0usize;
                            $(
                                if i == n {
                                    if skipped!($mode, self.d.$f) {
                                        continue;
                                    }
                                    return Some((Cow::Borrowed($name), &self.d.$f));
                                }
                                n += 1;
                            )*
                            let _ = n;
                            return None;
                        }
                    }
                }
                Fragment::Map(Box::new(S { d: self, i: 0 }))
            }
        }

        impl Deserialize for $ty {
            fn begin(out: &mut Option<Self>) -> &mut dyn Visitor {
                make_place!(Place);
                struct B<'a> {
                    $($f: Option<$t>,)*
                    out: &'a mut Option<$ty>,
                }
                impl de::Map for B<'_> {
                    fn key(&mut self, k: &str) -> Result<&mut dyn Visitor> {
                        Ok(match k {
                            $($name => Deserialize::begin(&mut self.$f),)*
                            _ => <dyn Visitor>::ignore(),
                        })
                    }
                    fn finish(&mut self) -> Result<()> {
                        *self.out = Some($ty { $($f: self.$f.take().ok_or(Error)?,)* });
                        Ok(())
                    }
                }
                impl Visitor for Place<$ty> {
                    fn map(&mut self) -> Result<Box<dyn de::Map + '_>> {
                        Ok(Box::new(B { $($f: init!($mode),)* out: &mut self.out }))
                    }
                }
                Place::new(out)
            }
        }
    };
}

object!(Config {
    machines: Vec<crate::model::Machine> = "machines" req,
    idle: Vec<ActionDef> = "idle" default,
});

object!(Session {
    key: String = "key" req,
    harness: String = "harness" req,
    host_session: Option<String> = "hostSession" default,
    cwd: String = "cwd" req,
    configs: Vec<String> = "configs" default,
    holding: Option<crate::record::InstanceKey> = "holding" default,
    suspended: Option<crate::record::InstanceKey> = "suspended" default,
    yielded: bool = "yielded" default,
    blocked: bool = "blocked" default,
    created: u64 = "created" req,
    last_active: u64 = "lastActive" req,
});

object!(Instance {
    id: String = "id" req,
    machine: String = "machine" req,
    r#ref: Option<String> = "ref" default,
    state: String = "state" req,
    status: crate::record::Status = "status" req,
    holder: Option<String> = "holder" default,
    version: u64 = "version" req,
    visits: SmallMap<u32> = "visits" default,
    interrupted: Option<String> = "interrupted" default,
    created: u64 = "created" req,
    updated: u64 = "updated" req,
});

object!(HistoryEntry {
    at: u64 = "at" req,
    session: String = "session" req,
    event: String = "event" req,
    from: Option<String> = "from" default,
    to: Option<String> = "to" default,
    params: SmallMap<String> = "params" default,
    trace: Vec<String> = "trace" default,
});

object!(MemoryStore {
    sessions: SmallMap<Session> = "sessions" req,
    bindings: SmallMap<String> = "bindings" req,
    instances: SmallMap<Instance> = "instances" req,
    history: SmallMap<Vec<HistoryEntry>> = "history" skip,
});

/// A struct variant's body (`{"state":…,"at_least":…}`: serde renames the
/// variant, not its fields).
struct VisitsBody {
    state: String,
    at_least: u32,
}
object!(VisitsBody { state: String = "state" req, at_least: u32 = "at_least" req });

struct HostBody {
    kind: String,
    params: SmallMap<Value>,
}
object!(HostBody { kind: String = "kind" req, params: SmallMap<Value> = "params" req });

/// `{k: v}`: an externally tagged variant.
struct One<'a, V> {
    k: &'static str,
    v: V,
    done: bool,
    _p: core::marker::PhantomData<&'a ()>,
}
impl<V: Serialize> ser::Map for One<'_, V> {
    fn next(&mut self) -> Option<(Cow<'_, str>, &dyn Serialize)> {
        if core::mem::replace(&mut self.done, true) {
            return None;
        }
        Some((Cow::Borrowed(self.k), &self.v))
    }
}
fn tagged<'a, V: Serialize + 'a>(k: &'static str, v: V) -> Fragment<'a> {
    Fragment::Map(Box::new(One {
        k,
        v,
        done: false,
        _p: core::marker::PhantomData,
    }))
}

/// A struct variant's fields, borrowed.
struct Fields<'a, const N: usize>([(&'static str, &'a dyn Serialize); N]);
impl<const N: usize> Serialize for Fields<'_, N> {
    fn begin(&self) -> Fragment<'_> {
        struct It<'a>(core::slice::Iter<'a, (&'static str, &'a dyn Serialize)>);
        impl ser::Map for It<'_> {
            fn next(&mut self) -> Option<(Cow<'_, str>, &dyn Serialize)> {
                self.0.next().map(|(k, v)| (Cow::Borrowed(*k), *v))
            }
        }
        Fragment::Map(Box::new(It(self.0.iter())))
    }
}

// `&String`: a `&str` cannot become the sized `&dyn Serialize` it is sent as.
#[allow(clippy::ptr_arg)]
fn host_fragment<'a>(kind: &'a String, params: &'a SmallMap<Value>) -> Fragment<'a> {
    tagged(
        "host",
        Fields([("kind", kind as &dyn Serialize), ("params", params)]),
    )
}

impl Serialize for Value {
    fn begin(&self) -> Fragment<'_> {
        match self {
            Value::Str(s) => Fragment::Str(Cow::Borrowed(s)),
            Value::Int(n) => Fragment::I64(*n),
            Value::Bool(b) => Fragment::Bool(*b),
            Value::List(l) => l.begin(),
        }
    }
}

impl Deserialize for Value {
    fn begin(out: &mut Option<Self>) -> &mut dyn Visitor {
        make_place!(Place);
        struct L<'a> {
            out: &'a mut Option<Value>,
            v: Vec<String>,
            el: Option<String>,
        }
        impl de::Seq for L<'_> {
            fn element(&mut self) -> Result<&mut dyn Visitor> {
                self.v.extend(self.el.take());
                Ok(Deserialize::begin(&mut self.el))
            }
            fn finish(&mut self) -> Result<()> {
                self.v.extend(self.el.take());
                *self.out = Some(Value::List(core::mem::take(&mut self.v)));
                Ok(())
            }
        }
        impl Visitor for Place<Value> {
            fn string(&mut self, s: &str) -> Result<()> {
                self.out = Some(Value::Str(s.into()));
                Ok(())
            }
            fn negative(&mut self, n: i64) -> Result<()> {
                self.out = Some(Value::Int(n));
                Ok(())
            }
            fn nonnegative(&mut self, n: u64) -> Result<()> {
                self.out = Some(Value::Int(i64::try_from(n).map_err(|_| Error)?));
                Ok(())
            }
            fn boolean(&mut self, b: bool) -> Result<()> {
                self.out = Some(Value::Bool(b));
                Ok(())
            }
            fn seq(&mut self) -> Result<Box<dyn de::Seq + '_>> {
                Ok(Box::new(L {
                    out: &mut self.out,
                    v: Vec::new(),
                    el: None,
                }))
            }
        }
        Place::new(out)
    }
}

impl Serialize for GuardDef {
    fn begin(&self) -> Fragment<'_> {
        match self {
            GuardDef::Visits { state, at_least } => tagged(
                "visits",
                Fields([("state", state as &dyn Serialize), ("at_least", at_least)]),
            ),
            GuardDef::Host { kind, params } => host_fragment(kind, params),
        }
    }
}

impl Deserialize for GuardDef {
    fn begin(out: &mut Option<Self>) -> &mut dyn Visitor {
        make_place!(Place);
        struct B<'a> {
            out: &'a mut Option<GuardDef>,
            visits: Option<VisitsBody>,
            host: Option<HostBody>,
            seen: bool,
        }
        impl de::Map for B<'_> {
            fn key(&mut self, k: &str) -> Result<&mut dyn Visitor> {
                if core::mem::replace(&mut self.seen, true) {
                    return Err(Error);
                }
                match k {
                    "visits" => Ok(Deserialize::begin(&mut self.visits)),
                    "host" => Ok(Deserialize::begin(&mut self.host)),
                    _ => Err(Error),
                }
            }
            fn finish(&mut self) -> Result<()> {
                *self.out = Some(match (self.visits.take(), self.host.take()) {
                    (Some(v), _) => GuardDef::Visits {
                        state: v.state,
                        at_least: v.at_least,
                    },
                    (_, Some(h)) => GuardDef::Host {
                        kind: h.kind,
                        params: h.params,
                    },
                    _ => return Err(Error),
                });
                Ok(())
            }
        }
        impl Visitor for Place<GuardDef> {
            fn map(&mut self) -> Result<Box<dyn de::Map + '_>> {
                Ok(Box::new(B {
                    out: &mut self.out,
                    visits: None,
                    host: None,
                    seen: false,
                }))
            }
        }
        Place::new(out)
    }
}

impl Serialize for Prompt {
    fn begin(&self) -> Fragment<'_> {
        match self {
            Prompt::Text(s) => tagged("text", s),
            Prompt::File(s) => tagged("file", s),
            Prompt::DefaultFile(s) => tagged("defaultFile", s),
        }
    }
}

impl Deserialize for Prompt {
    fn begin(out: &mut Option<Self>) -> &mut dyn Visitor {
        make_place!(Place);
        struct B<'a> {
            out: &'a mut Option<Prompt>,
            tag: u8,
            s: Option<String>,
        }
        impl de::Map for B<'_> {
            fn key(&mut self, k: &str) -> Result<&mut dyn Visitor> {
                if self.tag != 0 {
                    return Err(Error);
                }
                self.tag = match k {
                    "text" => 1,
                    "file" => 2,
                    "defaultFile" => 3,
                    _ => return Err(Error),
                };
                Ok(Deserialize::begin(&mut self.s))
            }
            fn finish(&mut self) -> Result<()> {
                let s = self.s.take().ok_or(Error)?;
                *self.out = Some(match self.tag {
                    1 => Prompt::Text(s),
                    2 => Prompt::File(s),
                    _ => Prompt::DefaultFile(s),
                });
                Ok(())
            }
        }
        impl Visitor for Place<Prompt> {
            fn map(&mut self) -> Result<Box<dyn de::Map + '_>> {
                Ok(Box::new(B {
                    out: &mut self.out,
                    tag: 0,
                    s: None,
                }))
            }
        }
        Place::new(out)
    }
}

impl Serialize for ActionDef {
    fn begin(&self) -> Fragment<'_> {
        match self {
            ActionDef::Prompt(p) => tagged("prompt", p),
            ActionDef::SetRef => Fragment::Str(Cow::Borrowed("setRef")),
            ActionDef::Host { kind, params } => host_fragment(kind, params),
        }
    }
}

impl Deserialize for ActionDef {
    fn begin(out: &mut Option<Self>) -> &mut dyn Visitor {
        make_place!(Place);
        struct B<'a> {
            out: &'a mut Option<ActionDef>,
            prompt: Option<Prompt>,
            host: Option<HostBody>,
            seen: bool,
        }
        impl de::Map for B<'_> {
            fn key(&mut self, k: &str) -> Result<&mut dyn Visitor> {
                if core::mem::replace(&mut self.seen, true) {
                    return Err(Error);
                }
                match k {
                    "prompt" => Ok(Deserialize::begin(&mut self.prompt)),
                    "host" => Ok(Deserialize::begin(&mut self.host)),
                    _ => Err(Error),
                }
            }
            fn finish(&mut self) -> Result<()> {
                *self.out = Some(match (self.prompt.take(), self.host.take()) {
                    (Some(p), _) => ActionDef::Prompt(p),
                    (_, Some(h)) => ActionDef::Host {
                        kind: h.kind,
                        params: h.params,
                    },
                    _ => return Err(Error),
                });
                Ok(())
            }
        }
        impl Visitor for Place<ActionDef> {
            fn string(&mut self, s: &str) -> Result<()> {
                if s != "setRef" {
                    return Err(Error);
                }
                self.out = Some(ActionDef::SetRef);
                Ok(())
            }
            fn map(&mut self) -> Result<Box<dyn de::Map + '_>> {
                Ok(Box::new(B {
                    out: &mut self.out,
                    prompt: None,
                    host: None,
                    seen: false,
                }))
            }
        }
        Place::new(out)
    }
}

#[cfg(all(test, feature = "serde"))]
mod tests {
    use super::*;
    use crate::record::{InstanceKey, Status};

    fn same<T>(v: &T)
    where
        T: Serialize
            + Deserialize
            + serde::Serialize
            + serde::de::DeserializeOwned
            + PartialEq
            + core::fmt::Debug,
    {
        let a = miniserde::json::to_string(v);
        let b = serde_json::to_string(v).unwrap();
        assert_eq!(a, b);
        let back: T = miniserde::json::from_str(&b).unwrap();
        assert_eq!(&back, v);
    }

    #[test]
    fn dev_json_is_the_same_both_ways() {
        let text = include_str!("../../../../packages/smllm-wasm/test/dev.json");
        let c: Config = serde_json::from_str(text).unwrap();
        same(&c);
        assert_eq!(miniserde::json::to_string(&c), text.trim_end());
    }

    #[test]
    fn records_are_the_same_both_ways() {
        let key = InstanceKey {
            machine: "m".into(),
            id: "i\u{e9}\"\n\u{1}\u{1F600}".into(),
        };
        let s = Session {
            key: "sm-1".into(),
            harness: "h".into(),
            host_session: Some("x".into()),
            cwd: "/w".into(),
            configs: vec!["a".into()],
            holding: Some(key.clone()),
            suspended: None,
            yielded: true,
            blocked: false,
            created: u64::MAX,
            last_active: 2,
        };
        let i = Instance {
            id: "i".into(),
            machine: "m".into(),
            r#ref: Some("GH-1".into()),
            state: "S".into(),
            status: Status::Parked,
            holder: None,
            version: 3,
            visits: [("S", 2u32)].into_iter().collect(),
            interrupted: None,
            created: 1,
            updated: 2,
        };
        let h = HistoryEntry {
            at: 1,
            session: "s".into(),
            event: "e".into(),
            from: None,
            to: Some("T".into()),
            params: [("p", String::from("v"))].into_iter().collect(),
            trace: vec!["t".into()],
        };
        // miniserde's hidden `Deserialize::default` makes `T::default()`
        // ambiguous wherever its trait is in scope.
        let mut m = <MemoryStore as Default>::default();
        m.sessions.insert("sm-1", s.clone());
        m.bindings.insert("h/x", "sm-1".into());
        m.instances.insert("m/i", i.clone());
        same(&m);
        m.history.insert("m/i", vec![h.clone()]);
        same(&m);
        same(&s);
        same(&i);
        same(&h);
        // Absent default fields read as serde reads them.
        let min = r#"{"key":"k","harness":"h","cwd":"c","created":1,"lastActive":2}"#;
        let a: Session = miniserde::json::from_str(min).unwrap();
        let b: Session = serde_json::from_str(min).unwrap();
        assert_eq!(a, b);
        let v = crate::model::Value::List(vec!["a".into()]);
        same(&v);
        same(&crate::model::Value::Int(-5));
    }

    #[test]
    fn every_guard_action_and_prompt_is_the_same_both_ways() {
        let params: SmallMap<Value> = [
            ("run", Value::List(vec!["cargo".into(), "test".into()])),
            ("cwd", Value::Str("sub".into())),
            ("timeoutSecs", Value::Int(i64::MAX)),
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
        same(&crate::model::Position::Before);
        same(&crate::model::Position::After);
        same(&crate::record::Status::Completed);
    }

    #[test]
    fn what_serde_rejects_miniserde_rejects() {
        fn both_reject<T: Deserialize + serde::de::DeserializeOwned>(json: &str) {
            assert!(
                serde_json::from_str::<T>(json).is_err(),
                "serde took {json}"
            );
            assert!(
                miniserde::json::from_str::<T>(json).is_err(),
                "miniserde took {json}"
            );
        }
        both_reject::<Value>("9223372036854775808");
        both_reject::<Value>("1.5");
        both_reject::<Value>("null");
        both_reject::<ActionDef>(r#""setref""#);
        both_reject::<ActionDef>(r#"{"shell":{}}"#);
        both_reject::<ActionDef>(r#"{"prompt":{"text":"a","file":"b"}}"#);
        both_reject::<Prompt>(r#"{"html":"a"}"#);
        both_reject::<Prompt>("{}");
        both_reject::<GuardDef>(r#"{"visits":{"state":"S"}}"#);
        both_reject::<GuardDef>(
            r#"{"visits":{"state":"S","at_least":1},"host":{"kind":"k","params":{}}}"#,
        );
        both_reject::<Session>(r#"{"key":"k"}"#);
        both_reject::<Config>(r#"{"machines":[]} x"#);
        both_reject::<Config>(r#"{"machines":[],}"#);
    }
}
