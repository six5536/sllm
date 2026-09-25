// Derived from sokf 9c93f37 crates/lib/sokf-core/src/error.rs
//! The crate's error type.

use std::path::PathBuf;

/// Everything that can go wrong in the kit.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A refusal: an unknown profile or part, or a file the kit must read
    /// that does not parse. Nothing is written when one is returned from
    /// `install`. A CLI reports it as a usage error (exit 2).
    #[error("{0}")]
    Harness(String),
    /// An I/O failure on `path`.
    #[error("{}: {source}", path.display())]
    Io {
        /// The path being operated on.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// A bug or an inconsistent profile.
    #[error("internal: {0}")]
    Internal(String),
}

impl Error {
    /// Build an [`Error::Io`] for `path` from a [`std::io::Error`].
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}

/// The crate's result type.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names_the_path_and_the_source() {
        let e = Error::io(
            "/tmp/x",
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        );
        let msg = e.to_string();
        assert!(msg.contains("/tmp/x") && msg.contains("denied"), "{msg}");
        assert!(std::error::Error::source(&e).is_some());
        assert_eq!(Error::Harness("no".into()).to_string(), "no");
        assert_eq!(Error::Internal("bug".into()).to_string(), "internal: bug");
    }
}
