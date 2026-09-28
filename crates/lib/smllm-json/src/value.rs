//! The parsed tree.

use alloc::string::String;
use alloc::vec::Vec;

/// A JSON value. Objects keep their fields in document order, duplicates
/// included ([`crate::Fields`] decides what a duplicate means).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonValue {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// An integer in `i64` range.
    Int(i64),
    /// An integer above `i64::MAX` that fits `u64`.
    UInt(u64),
    /// Any other number (a fraction, an exponent, `-0`, or too large), as
    /// written: kept as text so no float code is needed.
    Number(String),
    /// A string, unescaped.
    String(String),
    /// An array.
    Array(Vec<JsonValue>),
    /// An object: its fields in document order.
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    /// What this value is, for "expected …, found …" errors.
    pub fn kind(&self) -> &'static str {
        match self {
            JsonValue::Null => "null",
            JsonValue::Bool(_) => "a boolean",
            JsonValue::Int(_) | JsonValue::UInt(_) => "an integer",
            JsonValue::Number(_) => "a number",
            JsonValue::String(_) => "a string",
            JsonValue::Array(_) => "an array",
            JsonValue::Object(_) => "an object",
        }
    }
}
