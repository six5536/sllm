// Derived from sokf 9c93f37 crates/lib/sokf-core/src/report/mod.rs
//! Findings and the report: severities error / warning / info, the text
//! form `<path>:<line>: <level>: <message> (<authority>)` with the counts
//! line, and the JSON form.

mod collect;
mod finding;
mod text;

pub use collect::Report;
pub use finding::{Finding, Severity};
pub use text::report_text;
