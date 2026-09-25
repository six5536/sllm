//! The state machine file as written.

mod forms;
mod machine;

pub use forms::{OneOrMany, OrderedMap, Run, StringOr};
pub use machine::{
    ActionSrc, Actions, CommandParams, EventMeta, GuardSrc, InstanceMeta, MachineFile, MachineMeta,
    ParamsSchema, PositionSrc, PromptParams, PropSchema, RefMeta, SharedActionSrc, StateMeta,
    StateNode, StateType, TransitionSrc, Transitions, VisitsParams,
};
