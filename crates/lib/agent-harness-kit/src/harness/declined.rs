// Derived from sokf 9c93f37 crates/lib/sokf-core/src/harness/record.rs
//! The declined parts: a store behind a trait, and a TOML implementation
//! that keeps `[harness.<name>] without = [...]` in the tool's config file,
//! edited in place with its comments kept.

use std::{fs, path::PathBuf};

use toml_edit::{Array, DocumentMut, Item, Table, TableLike, Value};

use crate::{Error, Result, harness::read_text};

/// Where the parts a user declined are kept.
pub trait DeclinedStore {
    /// The declined parts of `harness`. A store that cannot be read is a
    /// refusal; `install` calls this before any write.
    fn declined(&self, harness: &str) -> Result<Vec<String>>;
    /// Set the declined parts of `harness` to `parts`, writing only on
    /// change. Called after every part is written.
    fn set_declined(&self, harness: &str, parts: &[String]) -> Result<()>;
}

/// A [`DeclinedStore`] in a TOML file: `[harness.<name>] without = [...]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TomlDeclined {
    /// The file, e.g. `.smllm/config.toml`.
    pub path: PathBuf,
    /// The file's name in a refusal.
    pub display: String,
}

impl TomlDeclined {
    /// The store in the file `path`, named `display` in refusals.
    pub fn new(path: impl Into<PathBuf>, display: impl Into<String>) -> Self {
        TomlDeclined {
            path: path.into(),
            display: display.into(),
        }
    }

    fn refuse(&self, m: &str) -> Error {
        Error::Harness(format!("{}: {m}", self.display))
    }

    fn parse(&self, text: &str) -> Result<DocumentMut> {
        text.parse()
            .map_err(|e: toml_edit::TomlError| self.refuse(e.message()))
    }
}

/// `Some(None)` for no item, `Some(Some(table))` for a table, `None` for
/// an item of another type.
fn table(item: Option<&Item>) -> Option<Option<&dyn TableLike>> {
    match item {
        None => Some(None),
        Some(i) => i.as_table_like().map(Some),
    }
}

impl DeclinedStore for TomlDeclined {
    fn declined(&self, harness: &str) -> Result<Vec<String>> {
        let Some(text) = read_text(&self.path)? else {
            return Ok(Vec::new());
        };
        let doc = self.parse(&text)?;
        let Some(h) =
            table(doc.get("harness")).ok_or_else(|| self.refuse("`harness` is not a table"))?
        else {
            return Ok(Vec::new());
        };
        let Some(t) = table(h.get(harness))
            .ok_or_else(|| self.refuse(&format!("`harness.{harness}` is not a table")))?
        else {
            return Ok(Vec::new());
        };
        let Some(without) = t.get("without") else {
            return Ok(Vec::new());
        };
        let not_list = || {
            self.refuse(&format!(
                "`harness.{harness}.without` is not a list of part names"
            ))
        };
        without
            .as_array()
            .ok_or_else(not_list)?
            .iter()
            .map(|v| v.as_str().map(str::to_string).ok_or_else(not_list))
            .collect()
    }

    fn set_declined(&self, harness: &str, parts: &[String]) -> Result<()> {
        let text = read_text(&self.path)?;
        let Some(out) = set_declined_text(text.as_deref(), harness, parts, &self.display)? else {
            return Ok(());
        };
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        }
        fs::write(&self.path, out).map_err(|e| Error::io(&self.path, e))
    }
}

/// The config text with `harness.<name>.without` set to `parts`, edited in
/// place; `None` when the text is unchanged. `display` names the file in a
/// refusal.
pub fn set_declined_text(
    text: Option<&str>,
    harness: &str,
    parts: &[String],
    display: &str,
) -> Result<Option<String>> {
    let refuse = |m: &str| Error::Harness(format!("{display}: {m}"));
    let mut doc: DocumentMut = match text {
        Some(t) => t
            .parse()
            .map_err(|e: toml_edit::TomlError| refuse(e.message()))?,
        None => DocumentMut::new(),
    };
    let harness_table = doc
        .entry("harness")
        .or_insert_with(|| {
            let mut t = Table::new();
            t.set_implicit(true);
            Item::Table(t)
        })
        .as_table_like_mut()
        .ok_or_else(|| refuse("`harness` is not a table"))?;
    let table = harness_table
        .entry(harness)
        .or_insert(Item::Table(Table::new()))
        .as_table_like_mut()
        .ok_or_else(|| refuse(&format!("`harness.{harness}` is not a table")))?;
    let mut array = Array::new();
    for part in parts {
        array.push(part.as_str());
    }
    table.insert("without", Item::Value(Value::Array(array)));
    let out = doc.to_string();
    Ok((Some(out.as_str()) != text).then_some(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    fn hooks() -> Vec<String> {
        vec!["hooks".to_string()]
    }

    fn without(text: &str) -> toml_edit::Item {
        let doc: DocumentMut = text.parse().unwrap();
        doc["harness"]["claude"]["without"].clone()
    }

    #[test]
    fn sets_the_declined_list_in_place() {
        let d = "config.toml";
        assert_eq!(
            set_declined_text(None, "claude", &hooks(), d).unwrap(),
            Some("[harness.claude]\nwithout = [\"hooks\"]\n".into())
        );
        let text = "# The machines.\n[machines]\nfiles = [\"a\"] # kept\n";
        let out = set_declined_text(Some(text), "claude", &hooks(), d)
            .unwrap()
            .unwrap();
        assert!(out.starts_with(text), "{out}");
        assert!(
            out.ends_with("[harness.claude]\nwithout = [\"hooks\"]\n"),
            "{out}"
        );
        let text = "[harness.claude] # mine\nwithout = [\"skills\"]\n";
        let out = set_declined_text(Some(text), "claude", &hooks(), d)
            .unwrap()
            .unwrap();
        assert_eq!(out, "[harness.claude] # mine\nwithout = [\"hooks\"]\n");
        assert_eq!(
            set_declined_text(Some(&out), "claude", &hooks(), d).unwrap(),
            None
        );
        let out = set_declined_text(
            Some("harness = { claude = { without = [] } }\n"),
            "claude",
            &hooks(),
            d,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            without(&out).as_array().unwrap().get(0).unwrap().as_str(),
            Some("hooks")
        );
        let e = set_declined_text(Some("harness = 1\n"), "claude", &hooks(), d).unwrap_err();
        assert!(e.to_string().contains("`harness` is not a table"), "{e}");
        let e =
            set_declined_text(Some("[harness]\nclaude = 1\n"), "claude", &hooks(), d).unwrap_err();
        assert!(
            e.to_string().contains("`harness.claude` is not a table"),
            "{e}"
        );
        let e = set_declined_text(Some("[harness\n"), "claude", &hooks(), d).unwrap_err();
        assert!(matches!(e, Error::Harness(_)), "{e}");
        assert!(e.to_string().starts_with("config.toml: "), "{e}");
    }

    #[test]
    fn the_toml_store_reads_and_writes() {
        let dir = temp_dir("declined-store");
        let store = TomlDeclined::new(dir.join("sub/config.toml"), "sub/config.toml");
        assert_eq!(store.declined("claude").unwrap(), Vec::<String>::new());
        store.set_declined("claude", &hooks()).unwrap();
        assert_eq!(store.declined("claude").unwrap(), hooks());
        assert_eq!(store.declined("other").unwrap(), Vec::<String>::new());
        let before = std::fs::metadata(&store.path).unwrap().modified().unwrap();
        store.set_declined("claude", &hooks()).unwrap();
        assert_eq!(
            std::fs::metadata(&store.path).unwrap().modified().unwrap(),
            before
        );
        for (text, empty) in [("[machines]\n", true), ("[harness.claude]\nx = 1\n", true)] {
            std::fs::write(&store.path, text).unwrap();
            assert_eq!(store.declined("claude").unwrap().is_empty(), empty);
        }
        for (text, why) in [
            ("harness = 1\n", "`harness` is not a table"),
            ("[harness]\nclaude = 1\n", "`harness.claude` is not a table"),
            (
                "[harness.claude]\nwithout = 1\n",
                "is not a list of part names",
            ),
            (
                "[harness.claude]\nwithout = [1]\n",
                "is not a list of part names",
            ),
            ("[harness\n", "sub/config.toml: "),
        ] {
            std::fs::write(&store.path, text).unwrap();
            let e = store.declined("claude").unwrap_err();
            assert!(e.to_string().contains(why), "{e}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
