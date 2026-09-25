//! JSON Schema of the state machine format, derived from the source types
//! (CFG-15).

use crate::source::MachineFile;

/// The JSON Schema, pretty-printed.
// @zen-impl: CFG-15_AC-1
pub fn json_schema() -> String {
    let schema = schemars::schema_for!(MachineFile);
    serde_json::to_string_pretty(&schema).unwrap_or_default()
}
