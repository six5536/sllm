//! One history line per event, appended by the store as JSONL (DEC-8).
// @zen-component: INST-Records

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::prelude::*;
use crate::utils::SmallMap;

/// A history entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct HistoryEntry {
    /// When, unix ms.
    pub at: u64,
    /// Session key.
    pub session: String,
    /// Event fired (`enter`, `submit`, …).
    pub event: String,
    /// State before.
    #[cfg_attr(feature = "serde", serde(default))]
    pub from: Option<String>,
    /// State after.
    #[cfg_attr(feature = "serde", serde(default))]
    pub to: Option<String>,
    /// The event's params (TURN-10: kept here, not on the instance).
    #[cfg_attr(feature = "serde", serde(default))]
    pub params: SmallMap<String>,
    /// Guard results, states passed through, failed actions.
    #[cfg_attr(feature = "serde", serde(default))]
    pub trace: Vec<String>,
}
