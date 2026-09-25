// Derived from sokf 9c93f37 crates/lib/sokf-core/src/harness/hook/guard.rs
//! The loop guard of a blocking hook: the hash of the last text the hook
//! blocked on, one file per key (a repository, a session) in a directory the
//! caller chooses. A hook that would block twice on the same text lets the
//! agent stop instead.

use std::{fs, path::PathBuf};

use crate::hash::{Fnv, hash_text};

/// A loop guard whose files live in one directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopGuard {
    dir: PathBuf,
}

impl LoopGuard {
    /// A guard keeping its files in `dir`, created on the first write with a
    /// `.gitignore` of `*` so a directory inside a repository stays out of
    /// it.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        LoopGuard { dir: dir.into() }
    }

    /// The guard file of `key`: the key itself when it is short and made of
    /// `[A-Za-z0-9_-]`, else its hash.
    fn path(&self, key: &str) -> PathBuf {
        let safe = !key.is_empty()
            && key.len() <= 64
            && key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        let name = if safe {
            key.to_string()
        } else {
            let mut h = Fnv::new();
            h.update(key.as_bytes());
            h.render().replace(':', "-")
        };
        self.dir.join(format!("{name}.hash"))
    }

    /// The hash last recorded for `key`; `None` when there is none.
    pub fn read(&self, key: &str) -> Option<String> {
        let text = fs::read_to_string(self.path(key)).ok()?;
        let text = text.trim().to_string();
        (!text.is_empty()).then_some(text)
    }

    /// Record `hash` for `key`. Silent on failure: the guard is a cache.
    pub fn write(&self, key: &str, hash: &str) {
        if fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let ignore = self.dir.join(".gitignore");
        if !ignore.exists() {
            let _ = fs::write(&ignore, "*\n");
        }
        let _ = fs::write(self.path(key), format!("{hash}\n"));
    }

    /// Forget `key`, e.g. once the hook no longer blocks.
    pub fn clear(&self, key: &str) {
        let _ = fs::remove_file(self.path(key));
    }

    /// Whether the hook should block on `text` for `key`: `false` when it
    /// blocked on the same text last time; otherwise `true`, and the text's
    /// hash is recorded.
    pub fn should_block(&self, key: &str, text: &str) -> bool {
        let hash = hash_text(text);
        if self.read(key).as_deref() == Some(hash.as_str()) {
            return false;
        }
        self.write(key, &hash);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    #[test]
    fn reads_what_it_wrote_per_key() {
        let dir = temp_dir("guard");
        let guard = LoopGuard::new(dir.join("cache"));
        assert_eq!(guard.read("repo"), None);
        guard.write("repo", "fnv1a64:01");
        assert_eq!(guard.read("repo").as_deref(), Some("fnv1a64:01"));
        assert_eq!(
            fs::read_to_string(dir.join("cache/.gitignore")).unwrap(),
            "*\n"
        );
        assert!(dir.join("cache/repo.hash").is_file());
        // Keys are separate; an unsafe key is hashed into a file name.
        assert_eq!(guard.read("other"), None);
        guard.write("a/../b c", "fnv1a64:02");
        assert_eq!(guard.read("a/../b c").as_deref(), Some("fnv1a64:02"));
        assert!(!dir.join("b c.hash").exists());
        guard.clear("repo");
        assert_eq!(guard.read("repo"), None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn blocks_once_per_text() {
        let dir = temp_dir("guard-block");
        let guard = LoopGuard::new(&dir);
        assert!(guard.should_block("s1", "2 errors"));
        assert!(!guard.should_block("s1", "2 errors"));
        assert!(!guard.should_block("s1", "2 errors\r\n".trim_end()));
        assert!(guard.should_block("s2", "2 errors"));
        assert!(guard.should_block("s1", "1 error"));
        assert!(guard.should_block("s1", "2 errors"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unwritable_directory_is_silent() {
        let dir = temp_dir("guard-unwritable");
        fs::write(dir.join("file"), "x").unwrap();
        let guard = LoopGuard::new(dir.join("file"));
        guard.write("k", "h");
        assert_eq!(guard.read("k"), None);
        // Never recorded, so it blocks every time.
        assert!(guard.should_block("k", "t"));
        assert!(guard.should_block("k", "t"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
