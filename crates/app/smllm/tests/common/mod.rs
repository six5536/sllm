//! A temporary world for end-to-end tests: a project dir and isolated user
//! dirs (XDG_*/HOME), with the real binary.
#![allow(dead_code)]

use std::io::{BufRead as _, BufReader, Lines, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout};
use std::sync::atomic::{AtomicUsize, Ordering};

use assert_cmd::Command;
use serde_json::{Value, json};

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

    /// User dirs under `root` on every OS: XDG_* and HOME, and on Windows
    /// (where etcetera uses Known Folders from the profile) USERPROFILE,
    /// APPDATA and LOCALAPPDATA.
    fn user_dirs(&self) -> [(&'static str, PathBuf); 6] {
        let home = self.root.join("home");
        [
            ("XDG_STATE_HOME", self.root.join("state")),
            ("XDG_CONFIG_HOME", self.root.join("config")),
            ("HOME", home.clone()),
            ("USERPROFILE", home.clone()),
            ("APPDATA", home.join("AppData/Roaming")),
            ("LOCALAPPDATA", home.join("AppData/Local")),
        ]
    }

    pub fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("smllm").unwrap();
        c.current_dir(&self.project)
            .envs(self.user_dirs())
            .env_remove("SMLLM_CONFIG");
        c
    }

    /// `smllm mcp` in this world, with a client over its stdio.
    pub fn mcp(&self) -> Mcp {
        let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("smllm"))
            .arg("mcp")
            .current_dir(&self.project)
            .envs(self.user_dirs())
            .env_remove("SMLLM_CONFIG")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        Mcp {
            stdin: child.stdin.take(),
            lines: BufReader::new(child.stdout.take().unwrap()).lines(),
            child,
            meta: None,
        }
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
    pub fn hook(&self, hook: &str, event: Value) -> Value {
        let o = self.run_stdin(&["harness", "hook", "claude", hook], &event.to_string());
        assert_eq!(o.code, 0, "{hook}: {}", o.stderr);
        serde_json::from_str(&o.stdout).unwrap()
    }

    /// Start Claude Code session `sid` here: the key of its session, in idle.
    pub fn start(&self, sid: &str) -> String {
        let v = self.hook("session-start", json!({ "session_id": sid, "cwd": self.project, "hook_event_name": "SessionStart", "source": "startup" }));
        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "SessionStart");
        let ctx = v["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap();
        assert!(ctx.contains("· idle"), "{ctx}");
        key_in(ctx)
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

/// A JSON-RPC client of `smllm mcp` over stdio.
pub struct Mcp {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Lines<BufReader<ChildStdout>>,
    /// Sent as every request's `_meta`.
    meta: Option<Value>,
}

impl Mcp {
    /// A 2026-07-28 client: no `initialize`; the version is in every
    /// request's `_meta`.
    pub fn at_2026_07_28(mut self) -> Self {
        self.meta = Some(json!({
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": {},
            "io.modelcontextprotocol/clientInfo": { "name": "t", "version": "1" }
        }));
        self
    }

    fn write(&mut self, msg: Value) {
        let stdin = self.stdin.as_mut().expect("stdin is open");
        writeln!(stdin, "{msg}").unwrap();
        stdin.flush().unwrap();
    }

    /// Send request `id` without waiting for the answer.
    pub fn send(&mut self, id: u64, method: &str, mut params: Value) {
        if let Some(m) = &self.meta {
            params["_meta"] = m.clone();
        }
        self.write(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
    }

    /// Send a notification.
    pub fn notify(&mut self, method: &str) {
        self.write(json!({ "jsonrpc": "2.0", "method": method }));
    }

    /// The server's next message.
    pub fn next(&mut self) -> Value {
        serde_json::from_str(&self.lines.next().unwrap().unwrap()).unwrap()
    }

    /// Send request `id` and wait for its answer.
    pub fn call(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(id, method, params);
        loop {
            let v = self.next();
            if v["id"] == id {
                return v;
            }
        }
    }

    /// Call the smllm tool: whether it is an error, and its text.
    pub fn tool(&mut self, id: u64, args: Value) -> (bool, String) {
        let r = self.call(
            id,
            "tools/call",
            json!({ "name": "smllm", "arguments": args }),
        );
        let text = r["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_default();
        (r["result"]["isError"] == true, text.to_string())
    }

    /// Close stdin: the server exits once it has answered everything.
    pub fn close(&mut self) {
        self.stdin = None;
    }

    /// Close stdin and expect a clean exit (which writes coverage data).
    pub fn finish(mut self) {
        self.close();
        let status = self.child.wait().unwrap();
        assert!(status.success(), "{status}");
    }
}

/// The session key in an `<smllm>` header.
pub fn key_in(text: &str) -> String {
    let at = text.find("session sm-").expect("a session header") + "session ".len();
    text[at..].split_whitespace().next().unwrap().to_string()
}
