//! sllm core: the library half of the sllm CLI.
//!
//! The binary (`crates/app/sllm`) owns argument parsing and rendering; the
//! logic it drives lives here, so it can be exercised without a process. This
//! is a skeleton, so today that logic is a single [`greet`] call — the shape to
//! follow when real functionality arrives, not the functionality itself.
//!
//! # Example
//!
//! ```
//! use sllm_core::greet;
//!
//! let greeting = greet(Some("world"))?;
//! assert_eq!(greeting.message, "Hello, world!");
//!
//! // A blank name is a config error rather than an empty greeting.
//! assert!(greet(Some("   ")).is_err());
//! # Ok::<(), sllm_core::Error>(())
//! ```

#![warn(missing_docs)]

pub mod error;
pub mod greeting;

pub use error::{Error, Result};
pub use greeting::{DEFAULT_NAME, Greeting, greet};
