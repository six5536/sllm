// Derived from sokf 9c93f37 crates/lib/sokf-core/src/harness/target.rs
//! The path of each part under a root directory.

use std::path::Path;

use crate::{
    Error, Result,
    harness::{Content, Part, Target},
};

/// The instructions file under `root`: `AGENTS.md` when it exists and
/// `CLAUDE.md` is absent or has a line `@AGENTS.md`; otherwise `CLAUDE.md`,
/// created when absent.
pub fn instructions_file(root: &Path) -> Result<&'static str> {
    let agents = root.join("AGENTS.md");
    let claude = root.join("CLAUDE.md");
    if !agents.is_file() {
        return Ok("CLAUDE.md");
    }
    if !claude.is_file() {
        return Ok("AGENTS.md");
    }
    let text = std::fs::read_to_string(&claude).map_err(|e| Error::io(&claude, e))?;
    Ok(if text.lines().any(|l| l.trim() == "@AGENTS.md") {
        "AGENTS.md"
    } else {
        "CLAUDE.md"
    })
}

/// The path `part` is written to, relative to `root` and `/`-separated; for
/// an external part, its location.
pub fn target_path(root: &Path, part: &Part) -> Result<String> {
    if let Content::External(ext) = &part.content {
        return Ok(ext.location());
    }
    Ok(match &part.target {
        Target::Fixed(p) => p.clone(),
        Target::Instructions => instructions_file(root)?.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::test_support::temp_dir;

    #[test]
    fn the_instructions_rule() {
        let dir = temp_dir("target-rule");
        let part = Part::instructions("instructions", "b");
        let t = || target_path(&dir, &part).unwrap();
        // Neither file: CLAUDE.md, created later.
        assert_eq!(t(), "CLAUDE.md");
        // AGENTS.md alone.
        fs::write(dir.join("AGENTS.md"), "# Agents\n").unwrap();
        assert_eq!(t(), "AGENTS.md");
        // Both, CLAUDE.md without the import.
        fs::write(dir.join("CLAUDE.md"), "# Claude\n").unwrap();
        assert_eq!(t(), "CLAUDE.md");
        // Both, with the import, spaces around it allowed.
        fs::write(dir.join("CLAUDE.md"), "  @AGENTS.md  \n").unwrap();
        assert_eq!(t(), "AGENTS.md");
        // CLAUDE.md alone.
        fs::remove_file(dir.join("AGENTS.md")).unwrap();
        assert_eq!(t(), "CLAUDE.md");
        // A fixed part ignores the tree.
        let hooks = Part::merge("hooks", ".claude/settings.json", vec![]);
        assert_eq!(target_path(&dir, &hooks).unwrap(), ".claude/settings.json");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unreadable_claude_md_is_an_io_error() {
        let dir = temp_dir("target-unreadable");
        fs::write(dir.join("AGENTS.md"), "a").unwrap();
        // A directory named CLAUDE.md is not a file: AGENTS.md alone.
        fs::create_dir(dir.join("CLAUDE.md")).unwrap();
        assert_eq!(instructions_file(&dir).unwrap(), "AGENTS.md");
        fs::remove_dir(dir.join("CLAUDE.md")).unwrap();
        // Invalid UTF-8 cannot be read as text.
        fs::write(dir.join("CLAUDE.md"), [0xff, 0xfe]).unwrap();
        assert!(matches!(instructions_file(&dir), Err(Error::Io { .. })));
        fs::remove_dir_all(&dir).unwrap();
    }
}
