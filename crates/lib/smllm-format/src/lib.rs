//! smllm-format: read, check and compile smllm state machine files.
//!
//! A state machine file is an XState v5 machine config written in YAML, with
//! smllm's data under `meta` (see [`source::MachineFile`]). [`load_machine`]
//! parses and validates one, collecting [`Finding`]s, and lowers it to the
//! [`smllm_core`] model; [`load_configs`] combines user and project
//! `config.toml`s; [`json_schema`] is the format's JSON Schema; [`compile`]
//! emits the model as JSON for wasm hosts.

#![warn(missing_docs)]

mod compile;
mod finding;
mod load;
mod locate;
mod lower;
mod schema;
mod shape;
pub mod source;
mod template;

pub use compile::compile;
pub use finding::{Finding, Findings, Level};
pub use load::{ConfigFile, Loaded, MachineSource, Origin, load_configs, load_machine};
pub use locate::locate;
pub use schema::json_schema;
pub use template::{config_template, machine_template};
