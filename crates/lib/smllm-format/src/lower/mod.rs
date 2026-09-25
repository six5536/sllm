//! Validation and lowering of a machine file to the core model.

mod actions;
mod checker;
mod events;
mod graph;
mod machine;

pub(crate) use actions::Files;
pub(crate) use checker::Checker;
pub(crate) use machine::lower;
