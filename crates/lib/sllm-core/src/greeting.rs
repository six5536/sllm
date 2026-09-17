//! The skeleton's stand-in for domain logic: building a greeting.
//!
//! It exists to give the binary something to call, the test suite something to
//! assert, and the release smoke tests something to observe. Replace it with
//! real modules; keep the shape — plain data in, a serializable value or an
//! [`Error`] out, no I/O and no printing.

use serde::Serialize;

use crate::{Error, Result};

/// Who a greeting addresses when the caller names nobody.
pub const DEFAULT_NAME: &str = "world";

/// A greeting, ready for the binary to render as text or JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Greeting {
    /// Who the greeting is addressed to, trimmed of surrounding whitespace.
    pub name: String,
    /// The rendered greeting line.
    pub message: String,
}

/// Build a [`Greeting`] for `name`, or for [`DEFAULT_NAME`] when it is `None`.
///
/// # Errors
///
/// Returns [`Error::Config`] when a name is given but is blank once trimmed:
/// `sllm hello ""` is a mistake worth reporting, not a greeting to nobody.
///
/// # Example
///
/// ```
/// # use sllm_core::{DEFAULT_NAME, greet};
/// assert_eq!(greet(None)?.name, DEFAULT_NAME);
/// assert_eq!(greet(Some("  ada  "))?.name, "ada");
/// # Ok::<(), sllm_core::Error>(())
/// ```
pub fn greet(name: Option<&str>) -> Result<Greeting> {
    let name = name.unwrap_or(DEFAULT_NAME).trim();
    if name.is_empty() {
        return Err(Error::config("name must not be blank"));
    }
    Ok(Greeting {
        name: name.to_string(),
        message: format!("Hello, {name}!"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greets_the_default_when_no_name_is_given() {
        let g = greet(None).unwrap();
        assert_eq!(g.name, DEFAULT_NAME);
        assert_eq!(g.message, "Hello, world!");
    }

    #[test]
    fn greets_a_named_recipient_and_trims_it() {
        assert_eq!(greet(Some("ada")).unwrap().message, "Hello, ada!");
        // Surrounding whitespace is the shell's, not the user's intent.
        assert_eq!(greet(Some("\t ada \n")).unwrap().name, "ada");
    }

    #[test]
    fn a_blank_name_is_an_error_rather_than_a_greeting_to_nobody() {
        for blank in ["", " ", "\t\n"] {
            let e = greet(Some(blank)).unwrap_err();
            assert!(matches!(e, Error::Config(_)), "{blank:?} -> {e}");
            assert!(e.to_string().contains("must not be blank"));
        }
    }

    #[test]
    fn serializes_to_the_json_the_cli_emits() {
        let json = serde_json::to_string(&greet(Some("ada")).unwrap()).unwrap();
        assert_eq!(json, r#"{"name":"ada","message":"Hello, ada!"}"#);
    }
}
