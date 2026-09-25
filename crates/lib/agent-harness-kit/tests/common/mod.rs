// Derived from sokf 9c93f37 crates/lib/sokf-core/tests/common/mod.rs
//! A test tool over a temporary tree.

#![allow(dead_code)]

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use agent_harness_kit::{
    DeclinedStore, Error, ExternalPart, MergeOp, Part, Profile, Result, Scope, TomlDeclined, Tool,
};
use serde_json::json;

pub const INSTRUCTIONS: &str = "This project uses tool.\nRead the tool skill first.\n";
pub const SKILL: &str = "---\nname: tool\n---\n\n# Tool\n";
pub const PREFIX: &str = "tool harness hook ";

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A temporary directory: the project at its root, the user's home under
/// `home/`.
pub struct TempTree {
    dir: PathBuf,
}

impl TempTree {
    pub fn empty(name: &str) -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ahk-it-{name}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        TempTree { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn write(&self, rel: &str, text: &str) {
        let p = self.dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    pub fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.dir.join(rel)).unwrap()
    }

    pub fn exists(&self, rel: &str) -> bool {
        self.dir.join(rel).exists()
    }

    /// Every file under the tree with its bytes.
    pub fn files(&self) -> BTreeMap<String, Vec<u8>> {
        let mut out = BTreeMap::new();
        walk(&self.dir, &self.dir, &mut out);
        out
    }

    pub fn tool(&self) -> TestTool {
        TestTool {
            dir: self.dir.clone(),
        }
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk(root, &path, out);
        } else {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel, fs::read(&path).unwrap());
        }
    }
}

/// The user-scope `mcp` part: stands in for `claude mcp add-json --scope
/// user` by keeping the server's JSON in a file.
#[derive(Debug)]
pub struct FakeMcp {
    pub file: PathBuf,
}

impl ExternalPart for FakeMcp {
    fn location(&self) -> String {
        "claude mcp (user)".into()
    }

    fn expected(&self) -> String {
        json!({"command": "tool", "args": ["mcp"]}).to_string()
    }

    fn observe(&self) -> Result<Option<String>> {
        match fs::read_to_string(&self.file) {
            Ok(t) => Ok(Some(t)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::io(&self.file, e)),
        }
    }

    fn write(&self) -> Result<()> {
        // External parts are written before the part files, so nothing else
        // has created the directory yet.
        if let Some(dir) = self.file.parent() {
            fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        }
        fs::write(&self.file, self.expected()).map_err(|e| Error::io(&self.file, e))
    }
}

/// The tool under test: named `tool`, with a `claude` profile per scope.
pub struct TestTool {
    pub dir: PathBuf,
}

pub fn hooks_ops() -> Vec<MergeOp> {
    ["SessionStart", "UserPromptSubmit", "Stop"]
        .iter()
        .map(|event| {
            let hook = match *event {
                "SessionStart" => "session-start",
                "UserPromptSubmit" => "user-prompt-submit",
                _ => "stop",
            };
            MergeOp::hook_command(*event, PREFIX, &format!("tool harness hook claude {hook}"))
        })
        .collect()
}

pub fn permissions_ops() -> Vec<MergeOp> {
    vec![MergeOp::array_entry("permissions.allow", "mcp__tool")]
}

pub fn mcp_ops() -> Vec<MergeOp> {
    vec![MergeOp::object_member(
        "mcpServers",
        "tool",
        json!({"command": "tool", "args": ["mcp"]}),
    )]
}

impl TestTool {
    fn user_root(&self) -> PathBuf {
        self.dir.join("home/.claude")
    }
}

impl Tool for TestTool {
    fn name(&self) -> &str {
        "tool"
    }

    fn profile(&self, harness: &str, scope: Scope) -> Option<Profile> {
        if harness != "claude" {
            return None;
        }
        let mcp = match scope {
            Scope::Project => Part::merge("mcp", ".mcp.json", mcp_ops()),
            Scope::User => Part::external(
                "mcp",
                Arc::new(FakeMcp {
                    file: self.dir.join("home/claude-mcp-user.json"),
                }),
            ),
        };
        Some(Profile {
            harness: "claude".into(),
            parts: vec![
                Part::files(
                    "skills",
                    ".claude/skills/tool",
                    vec![("SKILL.md".into(), SKILL.into())],
                ),
                Part::instructions("instructions", INSTRUCTIONS),
                mcp,
                Part::merge("hooks", ".claude/settings.json", hooks_ops()),
                Part::merge("permissions", ".claude/settings.json", permissions_ops()),
            ],
            hooks: vec![
                "session-start".into(),
                "user-prompt-submit".into(),
                "stop".into(),
            ],
        })
    }

    fn root(&self, scope: Scope) -> Result<PathBuf> {
        Ok(match scope {
            Scope::Project => self.dir.clone(),
            Scope::User => self.user_root(),
        })
    }

    fn record_path(&self, scope: Scope) -> Result<PathBuf> {
        Ok(match scope {
            Scope::Project => self.dir.join(".tool/harness.toml"),
            Scope::User => self.dir.join("home/.config/tool/harness.toml"),
        })
    }

    fn declined_store(&self, scope: Scope) -> Result<Box<dyn DeclinedStore + '_>> {
        Ok(Box::new(match scope {
            Scope::Project => {
                TomlDeclined::new(self.dir.join(".tool/config.toml"), ".tool/config.toml")
            }
            Scope::User => TomlDeclined::new(
                self.dir.join("home/.config/tool/config.toml"),
                "~/.config/tool/config.toml",
            ),
        }))
    }
}
