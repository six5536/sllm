//! The tool a harness integration belongs to: its name, its profiles and
//! where it keeps things. New in agent-harness-kit; sokf fixed these.

use std::{fmt, path::PathBuf, str::FromStr};

use serde::Serialize;

use crate::{
    Error, Result,
    harness::{DeclinedStore, Profile},
};

/// Where a harness integration is installed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// The project: files beside the project's own files.
    #[default]
    Project,
    /// The user: files in the user's harness configuration.
    User,
}

impl Scope {
    /// The scope's name, as `--scope` takes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Project => "project",
            Scope::User => "user",
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Scope {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "project" => Ok(Scope::Project),
            "user" => Ok(Scope::User),
            other => Err(Error::Harness(format!("no scope named `{other}`"))),
        }
    }
}

/// A CLI that plugs into agent harnesses. `install` and `status` are
/// generic over it.
pub trait Tool {
    /// The tool's name, e.g. `smllm`: the region markers are
    /// `<!-- smllm:harness -->` and `<!-- /smllm:harness -->`.
    fn name(&self) -> &str;

    /// The profile of `harness` at `scope`; `None` when there is none.
    fn profile(&self, harness: &str, scope: Scope) -> Option<Profile>;

    /// The directory the parts' relative paths resolve against, e.g. the
    /// project directory or `~/.claude`.
    fn root(&self, scope: Scope) -> Result<PathBuf>;

    /// The record file of `scope`.
    fn record_path(&self, scope: Scope) -> Result<PathBuf>;

    /// The store of the declined parts of `scope`.
    fn declined_store(&self, scope: Scope) -> Result<Box<dyn DeclinedStore + '_>>;

    /// The record file's first line.
    fn record_header(&self) -> String {
        format!("# Written by {} harness install. Do not edit.", self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_parse_and_print() {
        for s in [Scope::Project, Scope::User] {
            assert_eq!(s.as_str().parse::<Scope>().unwrap(), s);
            assert_eq!(s.to_string(), s.as_str());
        }
        assert_eq!(
            "global".parse::<Scope>().unwrap_err().to_string(),
            "no scope named `global`"
        );
        assert_eq!(serde_json::to_string(&Scope::User).unwrap(), "\"user\"");
        assert_eq!(Scope::default(), Scope::Project);
    }
}
