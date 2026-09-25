//! agent-harness-kit: shared plumbing for CLIs that plug into LLM agent
//! harnesses (Claude Code first), factored from sokf.
//!
//! - [`harness`]: profiles of parts the tool supplies, their states, the
//!   `file` / `region` / `merge` / `external` write kinds, the record, the
//!   declined parts, and [`install`] / [`status`] generic over a [`Tool`].
//! - [`hook`]: Claude Code hook input and answers, and the loop guard.
//! - [`report`]: findings (error / warning / info) and their text and JSON
//!   forms.
//! - [`cli`]: exit codes, stdout, broken pipes and the `error:` runner.
#![warn(missing_docs)]

mod error;
mod hash;
#[cfg(test)]
mod test_support;

pub mod cli;
pub mod harness;
pub mod hook;
pub mod report;

pub use error::{Error, Result};
pub use harness::{
    DeclinedStore, ExternalPart, HarnessResult, InstallOptions, MergeOp, Part, PartResult, Profile,
    Scope, State, TomlDeclined, Tool, install, status,
};
pub use hash::{hash_text, normalise};
pub use hook::{Answer, HookInput, LoopGuard};
pub use report::{Finding, Report, Severity, report_text};
