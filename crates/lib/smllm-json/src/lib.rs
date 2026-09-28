//! A small `no_std` JSON reader and writer, for wasm-sized builds.
//!
//! Reading parses text into a [`JsonValue`] tree ([`parse`]), then a type
//! takes its fields out of the tree ([`FromJson`], [`Fields`]). Writing goes
//! straight to a `String` ([`ToJson`], [`object`]), escaping strings as
//! serde_json does, so output is byte-identical to serde_json's for the same
//! shapes. Integers are `i64` / `u64`; any other number is kept as its text
//! and never becomes a float, so no float code is linked.
// @zen-component: ENG-Json
#![no_std]
#![warn(missing_docs)]

extern crate alloc;

mod convert;
mod error;
mod parse;
mod value;
mod write;

pub use convert::{Fields, FromJson, from_str};
pub use error::Error;
pub use parse::{MAX_DEPTH, parse};
pub use value::JsonValue;
pub use write::{ObjectWriter, ToJson, object, to_string, write_str};
