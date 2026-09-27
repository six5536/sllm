mod block;
mod text;

pub(crate) use block::{Block, push_safe};
pub use text::{AGENT_RULES, format_utc};
pub(crate) use text::{header, idle_header, quote};
