//! The app's error: usage and internal failures (exit 2).

use std::path::{Path, PathBuf};

/// A failure the CLI reports as `error: <message>` with exit 2.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// I/O at a path.
    #[error("{path}: {source}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// Anything else, as a message.
    #[error("{0}")]
    Msg(String),
    /// From the engine.
    #[error("{0}")]
    Engine(#[from] smllm_core::Error),
    /// From the harness kit.
    #[error("{0}")]
    Harness(#[from] agent_harness_kit::Error),
}

impl Error {
    /// I/O at `path`.
    pub fn io(path: &Path, source: std::io::Error) -> Self {
        Error::Io {
            path: path.to_path_buf(),
            source,
        }
    }

    /// A message.
    pub fn msg(m: impl Into<String>) -> Self {
        Error::Msg(m.into())
    }
}

/// App result.
pub type Result<T> = std::result::Result<T, Error>;
