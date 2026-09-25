// Derived from sokf 9c93f37 crates/lib/sokf-core/src/harness/mod.rs
//! Harness integration: profiles of parts the tool supplies, the state of
//! each part, the write kinds (`file`, `region`, `merge`, `external`), the
//! record, the declined parts, and `install` / `status`.

mod declined;
mod file;
mod install;
mod merge;
mod part;
mod record;
mod region;
mod state;
mod target;
mod tool;
mod write;

pub use declined::{DeclinedStore, TomlDeclined, set_declined_text};
pub use file::render_files;
pub use install::{HarnessResult, InstallOptions, PartResult, install, status};
pub use merge::{
    MergeOp, apply as apply_merge, extract, indent_of, json_text, parse_json,
    remove as remove_merge, render_merge, render_unmerge,
};
pub use part::{Content, ExternalPart, Part, Profile, Target};
pub use record::{Record, read_record, render_record};
pub use region::{Markers, find_region, render_region};
pub(crate) use state::read_text;
pub use state::{Found, Observed, State, expected, hash, observe, state};
pub use target::{instructions_file, target_path};
pub use tool::{Scope, Tool};
pub use write::{Plan, apply_plan, write_if_changed};
