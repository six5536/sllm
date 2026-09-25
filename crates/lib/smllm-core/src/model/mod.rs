//! The state machine model the engine runs (lowered from YAML by smllm-format).

mod action;
mod machine;

pub use action::{ActionDef, GuardDef, Prompt, Value};
pub use machine::{
    Config, EventDef, InstanceSpec, Machine, On, ParamSpec, Position, SharedAction, State,
    Transition,
};
