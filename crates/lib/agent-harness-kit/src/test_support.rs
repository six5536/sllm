//! Helpers for the unit tests.

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A fresh, empty temporary directory, unique per call and process.
pub(crate) fn temp_dir(name: &str) -> PathBuf {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("ahk-{name}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}
