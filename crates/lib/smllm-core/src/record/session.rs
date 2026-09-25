//! A session: one harness conversation, keyed by an smllm session key.
// @zen-component: STO-Records

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::prelude::*;
use crate::record::InstanceKey;

/// A session record (STO-1, STO-2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct Session {
    /// The session key (`sm-k7f3q2`).
    pub key: String,
    /// The harness that bound it (`claude`, `none`).
    pub harness: String,
    /// The harness's own session id, when it has one.
    #[cfg_attr(feature = "serde", serde(default))]
    pub host_session: Option<String>,
    /// Working directory at bind: guard/action commands run here (DEC-7).
    pub cwd: String,
    /// The config files bound at creation (STO-2); opaque to the core.
    #[cfg_attr(feature = "serde", serde(default))]
    pub configs: Vec<String>,
    /// The instance this session is working on; `None` = idle.
    #[cfg_attr(feature = "serde", serde(default))]
    pub holding: Option<InstanceKey>,
    /// The instance put aside by `unmatched` (IDLE: detour).
    #[cfg_attr(feature = "serde", serde(default))]
    pub suspended: Option<InstanceKey>,
    /// The agent fired `yield` since the last user prompt (TURN-4, TURN-8).
    #[cfg_attr(feature = "serde", serde(default))]
    pub yielded: bool,
    /// Created, unix ms.
    pub created: u64,
    /// Last call, unix ms.
    pub last_active: u64,
}
