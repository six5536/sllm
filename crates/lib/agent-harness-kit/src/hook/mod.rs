//! Hook adapters: Claude Code's hook input and answers, the writer that
//! keeps a failing hook from wedging the agent, and the loop guard.

mod answer;
mod guard;

pub use answer::{Answer, HookInput, emit};
pub use guard::LoopGuard;
