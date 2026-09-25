// Derived from sokf 9c93f37 crates/lib/sokf-core/src/harness/file.rs
//! The `file` kind: the tool owns the whole file.

use crate::{
    harness::{Content, Part},
    hash::normalise,
};

/// The files a `file` part writes, each with LF line endings; empty for a
/// part of another kind.
pub fn render_files(part: &Part) -> Vec<(String, String)> {
    match &part.content {
        Content::Files(files) => files
            .iter()
            .map(|(p, t)| (p.clone(), normalise(t)))
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_each_file_with_lf() {
        let part = Part::files("x", "d", vec![("a.md".into(), "a\r\nb\r\n".into())]);
        assert_eq!(
            render_files(&part),
            vec![("a.md".to_string(), "a\nb\n".to_string())]
        );
        assert!(render_files(&Part::instructions("i", "b")).is_empty());
    }
}
