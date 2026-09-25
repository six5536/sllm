//! A temporary world for end-to-end tests: a project dir and isolated user
//! dirs (XDG_*/HOME), with the real binary.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use assert_cmd::Command;

static NEXT: AtomicUsize = AtomicUsize::new(0);

pub struct World {
    pub root: PathBuf,
    pub project: PathBuf,
}

pub struct Out {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

impl World {
    pub fn new(name: &str) -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("smllm-e2e-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();
        Self { root, project }
    }

    /// A project with the showcase example as its config.
    pub fn showcase(name: &str) -> Self {
        let w = Self::new(name);
        copy_dir(&repo().join("examples/showcase"), &w.project.join(".smllm"));
        w
    }

    pub fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("smllm").unwrap();
        c.current_dir(&self.project)
            .env("XDG_STATE_HOME", self.root.join("state"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("HOME", self.root.join("home"))
            .env_remove("SMLLM_CONFIG");
        c
    }

    pub fn run(&self, args: &[&str]) -> Out {
        self.run_stdin(args, "")
    }

    pub fn run_stdin(&self, args: &[&str], stdin: &str) -> Out {
        let o = self
            .cmd()
            .args(args)
            .write_stdin(stdin.to_string())
            .output()
            .unwrap();
        Out {
            stdout: String::from_utf8(o.stdout).unwrap(),
            stderr: String::from_utf8(o.stderr).unwrap(),
            code: o.status.code().unwrap(),
        }
    }

    /// Call a Claude Code hook with a JSON event; the parsed stdout.
    pub fn hook(&self, hook: &str, event: serde_json::Value) -> serde_json::Value {
        let o = self.run_stdin(&["harness", "hook", "claude", hook], &event.to_string());
        assert_eq!(o.code, 0, "{hook}: {}", o.stderr);
        serde_json::from_str(&o.stdout).unwrap()
    }

    pub fn write(&self, rel: &str, text: &str) {
        let p = self.project.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    pub fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.project.join(rel)).unwrap()
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let p = e.unwrap().path();
        let dest = to.join(p.file_name().unwrap());
        if p.is_dir() {
            copy_dir(&p, &dest);
        } else {
            std::fs::copy(&p, &dest).unwrap();
        }
    }
}

/// The session key in an `<smllm>` header.
pub fn key_in(text: &str) -> String {
    let at = text.find("session sm-").expect("a session header") + "session ".len();
    text[at..].split_whitespace().next().unwrap().to_string()
}
