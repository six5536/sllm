// Derived from sokf 9c93f37 crates/lib/sokf-core/src/harness/write.rs
//! Whole-file reads and atomic writes that respect what the user set up:
//! a symlink stays a symlink, a file keeps its permissions.

use std::{
    fs::{self, File},
    io::Write as _,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{Error, Result};

/// Symlinks followed before giving up (the kernel's own limit is 40).
const MAX_LINKS: usize = 40;

static NEXT_TMP: AtomicU64 = AtomicU64::new(0);

/// The text of `path`; `None` when absent.
pub fn read_text(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(path, e)),
    }
}

/// Write `text` to `path` through a temp file and a rename, so a reader never
/// sees half a file, creating the parent directories. When `path` is a
/// symlink the file it points to is replaced, not the link; an existing
/// file's permissions carry over. The temp name is unique per write, so
/// concurrent writers never share one.
pub fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let target = resolve(path);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let n = NEXT_TMP.fetch_add(1, Ordering::Relaxed);
    let tmp = target.with_file_name(format!(".{name}.{}.{n}.tmp", std::process::id()));
    let written = fill(&tmp, &target, text)
        .and_then(|()| fs::rename(&tmp, &target).map_err(|e| Error::io(&target, e)));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written
}

/// Create `tmp` with `target`'s permissions (before any text lands in it),
/// then write `text`.
fn fill(tmp: &Path, target: &Path, text: &str) -> Result<()> {
    let mut f = File::create(tmp).map_err(|e| Error::io(tmp, e))?;
    if let Ok(meta) = fs::metadata(target) {
        f.set_permissions(meta.permissions())
            .map_err(|e| Error::io(tmp, e))?;
    }
    f.write_all(text.as_bytes()).map_err(|e| Error::io(tmp, e))
}

/// `path` with its symlinks followed, dangling ones included (the write then
/// creates the file the link names).
fn resolve(path: &Path) -> PathBuf {
    let mut p = path.to_path_buf();
    for _ in 0..MAX_LINKS {
        let Ok(link) = fs::read_link(&p) else { break };
        // A relative link is relative to the link's directory; `join` keeps
        // an absolute one as is.
        p = match p.parent() {
            Some(dir) => dir.join(link),
            None => link,
        };
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    #[test]
    fn reads_absent_as_none() {
        let dir = temp_dir("fs-read");
        assert_eq!(read_text(&dir.join("nope")).unwrap(), None);
        write_atomic(&dir.join("a/b.txt"), "x").unwrap();
        assert_eq!(
            read_text(&dir.join("a/b.txt")).unwrap().as_deref(),
            Some("x")
        );
        assert!(matches!(read_text(&dir), Err(Error::Io { .. })));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn leaves_no_temp_files() {
        let dir = temp_dir("fs-tmp");
        write_atomic(&dir.join("f"), "1").unwrap();
        write_atomic(&dir.join("f"), "2").unwrap();
        let names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(names, ["f"]);
        // A failed rename (onto a directory) removes its temp file.
        fs::create_dir(dir.join("d")).unwrap();
        fs::write(dir.join("d/x"), "").unwrap();
        assert!(write_atomic(&dir.join("d"), "3").is_err());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn writes_through_symlinks_and_keeps_the_mode() {
        use std::os::unix::fs::{PermissionsExt as _, symlink};
        let dir = temp_dir("fs-link");
        fs::write(dir.join("AGENTS.md"), "a\n").unwrap();
        symlink("AGENTS.md", dir.join("CLAUDE.md")).unwrap();
        write_atomic(&dir.join("CLAUDE.md"), "b\n").unwrap();
        assert!(
            fs::symlink_metadata(dir.join("CLAUDE.md"))
                .unwrap()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(dir.join("AGENTS.md")).unwrap(), "b\n");
        // A dangling link gets its file created.
        symlink(dir.join("sub/new.toml"), dir.join("cfg.toml")).unwrap();
        write_atomic(&dir.join("cfg.toml"), "c\n").unwrap();
        assert!(
            fs::symlink_metadata(dir.join("cfg.toml"))
                .unwrap()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(dir.join("sub/new.toml")).unwrap(), "c\n");
        // A private file stays private.
        let secret = dir.join("settings.json");
        fs::write(&secret, "{}").unwrap();
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o600)).unwrap();
        write_atomic(&secret, "{\"a\":1}").unwrap();
        let mode = fs::metadata(&secret).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        fs::remove_dir_all(&dir).unwrap();
    }
}
