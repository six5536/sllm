mod block;
mod text;

pub(crate) use block::Block;
pub use text::{AGENT_RULES, format_utc};
pub(crate) use text::{header, idle_header, quote};
