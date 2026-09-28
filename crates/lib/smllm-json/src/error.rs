//! The one error: what went wrong, and where.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use core::fmt;

/// A parse or conversion failure. [`Error::path`] names the field that
/// failed (`machines[0].states[2].name`); a parse failure names the byte
/// offset in its message instead.
///
/// Boxed: every conversion returns `Result<T, Error>`, and a one-pointer
/// error keeps those results small to move (8.8 KB less wasm than two
/// inline strings).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(Box<Inner>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Inner {
    path: String,
    message: String,
}

impl Error {
    /// A failure with no path yet.
    pub fn new(message: impl Into<String>) -> Self {
        Self(Box::new(Inner {
            path: String::new(),
            message: message.into(),
        }))
    }

    /// A parse failure at byte `offset`.
    pub(crate) fn at_byte(message: &str, offset: usize) -> Self {
        Self::new(format!("{message} at byte {offset}"))
    }

    /// Where the failure is (`machines[0].id`); empty at the top.
    pub fn path(&self) -> &str {
        &self.0.path
    }

    /// What failed.
    pub fn message(&self) -> &str {
        &self.0.message
    }

    /// The same error, one object field further out.
    pub fn in_field(self, key: &str) -> Self {
        self.within(key)
    }

    /// The same error, one array element further out.
    pub fn in_item(self, index: usize) -> Self {
        self.within(&format!("[{index}]"))
    }

    fn within(mut self, outer: &str) -> Self {
        let path = &mut self.0.path;
        let sep = if path.is_empty() || path.starts_with('[') {
            ""
        } else {
            "."
        };
        *path = format!("{outer}{sep}{path}");
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path().is_empty() {
            f.write_str(self.message())
        } else {
            write!(f, "{}: {}", self.path(), self.message())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn paths_read_outside_in() {
        let e = Error::new("expected a string")
            .in_field("name")
            .in_item(2)
            .in_field("states")
            .in_item(0)
            .in_field("machines");
        assert_eq!(e.path(), "machines[0].states[2].name");
        assert_eq!(e.message(), "expected a string");
        assert_eq!(
            e.to_string(),
            "machines[0].states[2].name: expected a string"
        );
        assert_eq!(Error::new("bad").to_string(), "bad");
    }
}
