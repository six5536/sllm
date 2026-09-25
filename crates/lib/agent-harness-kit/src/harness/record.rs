// Derived from sokf 9c93f37 crates/lib/sokf-core/src/harness/record.rs
//! The record: for each installed harness, the hash of each part the tool
//! wrote. A TOML file whose path and header line the tool supplies.

use std::{collections::BTreeMap, path::Path};

use toml_edit::DocumentMut;

use crate::{Error, Result, harness::read_text};

/// The record: harness name to part name to hash.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    /// One table per installed harness.
    pub harnesses: BTreeMap<String, BTreeMap<String, String>>,
}

/// Read the record at `path`; empty when absent, a refusal naming
/// `display` when it does not parse or holds anything but tables of
/// strings.
pub fn read_record(path: &Path, display: &str) -> Result<Record> {
    let Some(text) = read_text(path)? else {
        return Ok(Record::default());
    };
    let refuse = |m: &str| Error::Harness(format!("{display}: {m}"));
    let doc: DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| refuse(e.message()))?;
    let mut harnesses = BTreeMap::new();
    for (name, item) in doc.iter() {
        let table = item
            .as_table_like()
            .ok_or_else(|| refuse(&format!("`{name}` is not a table")))?;
        let mut parts = BTreeMap::new();
        for (part, value) in table.iter() {
            let hash = value
                .as_str()
                .ok_or_else(|| refuse(&format!("`{name}.{part}` is not a string")))?;
            parts.insert(part.to_string(), hash.to_string());
        }
        harnesses.insert(name.to_string(), parts);
    }
    Ok(Record { harnesses })
}

/// The record file's text: `header`, then one table per harness in name
/// order, keys in name order, LF.
pub fn render_record(record: &Record, header: &str) -> String {
    let mut out = format!("{header}\n");
    for (name, parts) in &record.harnesses {
        if parts.is_empty() {
            continue;
        }
        out.push_str(&format!("\n[{name}]\n"));
        for (part, hash) in parts {
            out.push_str(&format!("{part} = \"{hash}\"\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::test_support::temp_dir;

    const HEADER: &str = "# Written by tool harness install. Do not edit.";

    #[test]
    fn reads_and_renders_the_record() {
        let dir = temp_dir("record-rw");
        let path = dir.join("harness.toml");
        assert_eq!(
            read_record(&path, "harness.toml").unwrap(),
            Record::default()
        );
        let mut record = Record::default();
        record.harnesses.insert(
            "claude".into(),
            BTreeMap::from([
                ("skills".to_string(), "fnv1a64:0000000000000001".to_string()),
                ("hooks".to_string(), "fnv1a64:0000000000000002".to_string()),
            ]),
        );
        record.harnesses.insert(
            "aider".into(),
            BTreeMap::from([("x".to_string(), "fnv1a64:03".to_string())]),
        );
        let text = render_record(&record, HEADER);
        assert_eq!(
            text,
            "# Written by tool harness install. Do not edit.\n\n[aider]\nx = \"fnv1a64:03\"\n\n[claude]\nhooks = \"fnv1a64:0000000000000002\"\nskills = \"fnv1a64:0000000000000001\"\n"
        );
        fs::write(&path, &text).unwrap();
        assert_eq!(read_record(&path, "harness.toml").unwrap(), record);
        // An empty table renders nothing; a broken file is a refusal.
        record.harnesses.insert("empty".into(), BTreeMap::new());
        assert_eq!(render_record(&record, HEADER), text);
        for (bad, why) in [
            ("[claude]\nskills = 1\n", "`claude.skills` is not a string"),
            ("claude = 1\n", "`claude` is not a table"),
            ("[claude\n", ""),
        ] {
            fs::write(&path, bad).unwrap();
            let e = read_record(&path, "harness.toml").unwrap_err();
            assert!(matches!(e, Error::Harness(_)), "{e}");
            assert!(e.to_string().starts_with("harness.toml: "), "{e}");
            assert!(e.to_string().contains(why), "{e}");
        }
        fs::remove_dir_all(&dir).unwrap();
    }
}
