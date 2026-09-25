// Derived from sokf 9c93f37 crates/lib/sokf-core/src/harness/profile.rs
//! Profiles and parts: data the tool supplies, one profile per harness and
//! scope. The kit embeds no content.

use std::{fmt, sync::Arc};

use crate::{Result, harness::MergeOp};

/// A part the kit does not write as a file: the tool reads and writes it
/// itself, e.g. by running `claude mcp add-json --scope user`.
pub trait ExternalPart: fmt::Debug + Send + Sync {
    /// Where the part lives, as the report shows it, e.g. `claude mcp (user)`.
    fn location(&self) -> String;
    /// The content the tool wants, compared with what [`observe`] returns
    /// (line endings aside) and hashed into the record.
    ///
    /// [`observe`]: ExternalPart::observe
    fn expected(&self) -> String;
    /// What is there now; `None` when the part is absent.
    fn observe(&self) -> Result<Option<String>>;
    /// Make the part hold [`expected`](ExternalPart::expected).
    fn write(&self) -> Result<()>;
}

/// A part's content, in the shape its write kind uses.
#[derive(Debug, Clone)]
pub enum Content {
    /// `file` kind: the tool owns these files of a directory, each a path
    /// relative to it and its text. Written with LF line endings.
    Files(Vec<(String, String)>),
    /// `region` kind: the tool owns one block between its markers in the
    /// user's file. The block, without markers.
    Block(String),
    /// `merge` kind: the tool owns entries in the user's JSON file.
    Merge(Vec<MergeOp>),
    /// `external` kind: the tool reads and writes the part itself.
    External(Arc<dyn ExternalPart>),
}

/// Where a part lives, relative to the scope's root directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A path the profile fixes, relative to the root, `/`-separated.
    Fixed(String),
    /// The instructions file, chosen by the rule of
    /// [`instructions_file`](crate::harness::instructions_file).
    Instructions,
}

/// One part of a profile.
#[derive(Debug, Clone)]
pub struct Part {
    /// The name, as `--without` and the report use it.
    pub name: String,
    /// Where it lives. Ignored for [`Content::External`].
    pub target: Target,
    /// The content the tool wants there.
    pub content: Content,
}

impl Part {
    /// A `file` part: `files` (relative path, text) under the directory
    /// `dir`.
    pub fn files(
        name: impl Into<String>,
        dir: impl Into<String>,
        files: Vec<(String, String)>,
    ) -> Self {
        Part {
            name: name.into(),
            target: Target::Fixed(dir.into()),
            content: Content::Files(files),
        }
    }

    /// A `region` part in the instructions file (`AGENTS.md` or
    /// `CLAUDE.md`, per [`instructions_file`](crate::harness::instructions_file)).
    pub fn instructions(name: impl Into<String>, block: impl Into<String>) -> Self {
        Part {
            name: name.into(),
            target: Target::Instructions,
            content: Content::Block(block.into()),
        }
    }

    /// A `region` part in a fixed file.
    pub fn region(
        name: impl Into<String>,
        file: impl Into<String>,
        block: impl Into<String>,
    ) -> Self {
        Part {
            name: name.into(),
            target: Target::Fixed(file.into()),
            content: Content::Block(block.into()),
        }
    }

    /// A `merge` part: `ops` applied to the JSON object in `file`.
    pub fn merge(name: impl Into<String>, file: impl Into<String>, ops: Vec<MergeOp>) -> Self {
        Part {
            name: name.into(),
            target: Target::Fixed(file.into()),
            content: Content::Merge(ops),
        }
    }

    /// An `external` part.
    pub fn external(name: impl Into<String>, part: Arc<dyn ExternalPart>) -> Self {
        Part {
            name: name.into(),
            target: Target::Fixed(part.location()),
            content: Content::External(part),
        }
    }
}

/// A harness profile at one scope: its parts in profile order, and the
/// hooks its `hook` command accepts.
#[derive(Debug, Clone)]
pub struct Profile {
    /// The harness name, as `<NAME>` on the command line, e.g. `claude`.
    pub harness: String,
    /// The parts, in profile order.
    pub parts: Vec<Part>,
    /// The hook names `<tool> harness hook <NAME> <HOOK>` accepts.
    pub hooks: Vec<String>,
}

impl Profile {
    /// The part named `name`.
    pub fn part(&self, name: &str) -> Option<&Part> {
        self.parts.iter().find(|p| p.name == name)
    }

    /// Whether `hook` is one of the profile's hooks.
    pub fn has_hook(&self, hook: &str) -> bool {
        self.hooks.iter().any(|h| h == hook)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Ext;

    impl ExternalPart for Ext {
        fn location(&self) -> String {
            "ext (user)".into()
        }
        fn expected(&self) -> String {
            "x".into()
        }
        fn observe(&self) -> Result<Option<String>> {
            Ok(None)
        }
        fn write(&self) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn constructors_set_target_and_content() {
        let p = Profile {
            harness: "claude".into(),
            parts: vec![
                Part::files(
                    "skills",
                    ".claude/skills/t",
                    vec![("SKILL.md".into(), "s".into())],
                ),
                Part::instructions("instructions", "b"),
                Part::region("notes", "NOTES.md", "n"),
                Part::merge("hooks", ".claude/settings.json", vec![]),
                Part::external("mcp", Arc::new(Ext)),
            ],
            hooks: vec!["stop".into()],
        };
        assert_eq!(
            p.part("skills").unwrap().target,
            Target::Fixed(".claude/skills/t".into())
        );
        assert_eq!(p.part("instructions").unwrap().target, Target::Instructions);
        assert_eq!(
            p.part("notes").unwrap().target,
            Target::Fixed("NOTES.md".into())
        );
        assert!(matches!(
            p.part("hooks").unwrap().content,
            Content::Merge(_)
        ));
        assert_eq!(
            p.part("mcp").unwrap().target,
            Target::Fixed("ext (user)".into())
        );
        assert!(p.part("nope").is_none());
        assert!(p.has_hook("stop") && !p.has_hook("start"));
    }
}
