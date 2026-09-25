//! The CLI host: command guards and actions, prompt files, regex patterns,
//! the clock and randomness (DEC-4..9, ACT-4).
// @zen-component: DEC-CommandRunner

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use smllm_core::host::{Action, Call, Clock, Guard, Ids, InstructionSource, Matcher, Outcome};
use smllm_core::model::Value;
use wait_timeout::ChildExt as _;

/// `timeoutSecs` default (DEC-5).
pub const DEFAULT_TIMEOUT_SECS: u64 = 60;
/// Output tail kept for the trace.
const TAIL_CHARS: usize = 400;

/// Runs `command` guards and actions.
#[derive(Debug, Default, Clone, Copy)]
pub struct Commands;

/// The last `TAIL_CHARS` of `text`, on one line.
fn tail(text: &str) -> String {
    let t = text.trim();
    let start = t.char_indices().rev().nth(TAIL_CHARS).map_or(0, |(i, _)| i);
    let s = &t[start..];
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if start > 0 { format!("…{s}") } else { s }
}

/// Run a `command` call: string → system shell, list → exec; env only;
/// timeout; exit 0 = ok (DEC-4..6, DEC-9).
// @zen-impl: DEC-4_AC-1
// @zen-impl: DEC-5_AC-1
// @zen-impl: DEC-7_AC-1
pub fn run_command(call: &Call<'_>) -> Outcome {
    let mut cmd = match call.params.get("run") {
        Some(Value::Str(s)) => {
            let mut c = if cfg!(windows) {
                Command::new("cmd")
            } else {
                Command::new("sh")
            };
            if cfg!(windows) {
                c.args(["/C", s]);
            } else {
                c.args(["-c", s]);
            }
            c
        }
        Some(Value::List(argv)) if !argv.is_empty() => {
            let mut c = Command::new(&argv[0]);
            c.args(&argv[1..]);
            c
        }
        _ => {
            return Outcome {
                ok: false,
                detail: "no run param".into(),
            };
        }
    };
    let cwd = call
        .params
        .get("cwd")
        .and_then(Value::as_str)
        .unwrap_or(call.cwd);
    if !cwd.is_empty() {
        cmd.current_dir(cwd);
    }
    cmd.envs(call.env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    // Its own process group, so a timeout kills what it started too.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let secs = call
        .params
        .get("timeoutSecs")
        .and_then(Value::as_int)
        .and_then(|n| u64::try_from(n).ok())
        .unwrap_or(DEFAULT_TIMEOUT_SECS);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Outcome {
                ok: false,
                detail: format!("could not start: {e}"),
            };
        }
    };
    // Drain both pipes on threads so a chatty command cannot fill a pipe and
    // stall before the timeout. Output is collected as it arrives: a
    // background process the command left running keeps the pipes open, and
    // must not hold smllm hostage after the command itself exits (DEC-5).
    let collected = Arc::new(Mutex::new(Vec::new()));
    let drain = |r: Option<Box<dyn Read + Send>>| {
        let sink = Arc::clone(&collected);
        std::thread::spawn(move || {
            let Some(mut r) = r else { return };
            let mut buf = [0u8; 4096];
            while let Ok(n) = r.read(&mut buf) {
                if n == 0 {
                    break;
                }
                if let Ok(mut v) = sink.lock() {
                    v.extend_from_slice(&buf[..n]);
                }
            }
        })
    };
    let readers = [
        drain(
            child
                .stdout
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
        ),
        drain(
            child
                .stderr
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
        ),
    ];
    let status = match child.wait_timeout(Duration::from_secs(secs)) {
        Ok(Some(s)) => s,
        Ok(None) => {
            kill_tree(&mut child);
            return Outcome {
                ok: false,
                detail: format!("timed out after {secs}s"),
            };
        }
        Err(e) => {
            return Outcome {
                ok: false,
                detail: format!("wait failed: {e}"),
            };
        }
    };
    // Give the readers a moment to reach end-of-file, then take what arrived.
    let deadline = std::time::Instant::now() + Duration::from_millis(200);
    while readers.iter().any(|r| !r.is_finished()) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let text = collected
        .lock()
        .map(|v| String::from_utf8_lossy(&v).into_owned())
        .unwrap_or_default();
    let t = tail(&text);
    if status.success() {
        return Outcome {
            ok: true,
            detail: String::new(),
        };
    }
    let code = status.code().map_or_else(
        || "killed by a signal".to_string(),
        |c| format!("exited {c}"),
    );
    Outcome {
        ok: false,
        detail: if t.is_empty() {
            code
        } else {
            format!("{code}: {t}")
        },
    }
}

/// Kill a timed-out command and, on unix, its whole process group.
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    let _ = Command::new("kill")
        .args(["-KILL", &format!("-{}", child.id())])
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
    let _ = child.wait();
}

impl Guard for Commands {
    fn supports(&self, kind: &str) -> bool {
        kind == "command"
    }
    fn check(&mut self, call: &Call<'_>) -> Outcome {
        run_command(call)
    }
}

impl Action for Commands {
    fn supports(&self, kind: &str) -> bool {
        kind == "command"
    }
    fn run(&mut self, call: &Call<'_>) -> Outcome {
        run_command(call)
    }
}

/// Prompt files from disk, read at request time (ENG-4).
#[derive(Debug, Default, Clone, Copy)]
pub struct Files;

impl InstructionSource for Files {
    fn read(&self, file: &str) -> Result<Option<String>, String> {
        match std::fs::read_to_string(file) {
            Ok(t) => Ok(Some(t)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
}

impl Matcher for Files {
    fn is_match(&self, pattern: &str, value: &str) -> Result<bool, String> {
        regex::Regex::new(pattern)
            .map(|r| r.is_match(value))
            .map_err(|e| e.to_string())
    }
}

impl Clock for Files {
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64)
    }
}

/// OS randomness.
#[derive(Debug, Default, Clone, Copy)]
pub struct OsIds;

impl Ids for OsIds {
    fn random(&mut self) -> u64 {
        getrandom::u64().unwrap_or_else(|_| Files.now_ms() ^ u64::from(std::process::id()) << 32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smllm_core::SmallMap;

    fn call<'a>(params: &'a SmallMap<Value>, env: &'a [(String, String)]) -> Call<'a> {
        Call {
            machine: "m",
            kind: "command",
            params,
            env,
            cwd: "",
        }
    }

    #[test]
    fn shell_exec_env_timeout_and_failure() {
        let env = vec![("SMLLM_REF".to_string(), "GH-1".to_string())];
        let mut p = SmallMap::new();
        p.insert("run", Value::Str("test \"$SMLLM_REF\" = GH-1".into()));
        assert!(run_command(&call(&p, &env)).ok);
        p.insert("run", Value::Str("echo boom >&2; exit 3".into()));
        let o = run_command(&call(&p, &env));
        assert_eq!(o.detail, "exited 3: boom");
        p.insert("run", Value::List(vec!["true".into()]));
        assert!(run_command(&call(&p, &env)).ok);
        p.insert(
            "run",
            Value::List(vec!["definitely-not-a-command-xyz".into()]),
        );
        assert!(
            run_command(&call(&p, &env))
                .detail
                .starts_with("could not start")
        );
        // A background process holding the pipes does not stall the result.
        p.insert("run", Value::Str("sleep 5 & exit 0".into()));
        let t0 = std::time::Instant::now();
        assert!(run_command(&call(&p, &env)).ok);
        assert!(t0.elapsed() < std::time::Duration::from_secs(3));
        p.insert("run", Value::Str("sleep 5".into()));
        p.insert("timeoutSecs", Value::Int(1));
        assert_eq!(run_command(&call(&p, &env)).detail, "timed out after 1s");
        let empty = SmallMap::new();
        assert!(!run_command(&call(&empty, &env)).ok);
        let mut c = Commands;
        assert!(Guard::supports(&c, "command") && !Action::supports(&c, "x"));
        let mut p = SmallMap::new();
        p.insert("run", Value::Str("pwd".into()));
        p.insert("cwd", Value::Str("/".into()));
        assert!(Action::run(&mut c, &call(&p, &env)).ok);
        assert!(Guard::check(&mut c, &call(&p, &env)).ok);
    }

    #[test]
    fn tails_are_short_single_lines() {
        assert_eq!(tail(" a\nb \n"), "a b");
        let long = "x".repeat(1000);
        let t = tail(&long);
        assert!(t.starts_with('…') && t.chars().count() <= TAIL_CHARS + 2);
    }

    #[test]
    fn files_patterns_clock_ids() {
        assert_eq!(Files.read("/definitely/not/here").unwrap(), None);
        assert!(Files.read("/").is_err());
        assert!(Files.is_match("^GH-\\d+$", "GH-12").unwrap());
        assert!(!Files.is_match("^GH-\\d+$", "x").unwrap());
        assert!(Files.is_match("(", "x").is_err());
        assert!(Files.now_ms() > 0);
        let mut ids = OsIds;
        assert_ne!(ids.random(), ids.random());
    }
}
