//! What a host supplies: storage, guard/action runners, prompt files, pattern
//! matching, time and randomness (NFR-1).

mod memory;
mod traits;

pub use memory::MemoryStore;
pub use traits::{
    Action, Call, Clock, Guard, Host, HostError, Ids, InstructionSource, Matcher, Outcome, Store,
};
