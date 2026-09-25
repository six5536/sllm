//! `fire`, `session list|show`, `instance list|show` (CLI-4..6).
// @zen-component: CLI-Commands

use std::collections::HashMap;
use std::path::Path;

use serde_json::json;
use smllm_core::host::Store;
use smllm_core::{Bind, format_utc};

use crate::cli::{FireArgs, JsonArgs, KeyArgs};
use crate::error::{Error, Result};
use crate::output::{self, EXIT_ERRORS, EXIT_OK};
use crate::paths;
use crate::runtime::Runtime;
use crate::store::FsStore;

/// `KEY=VALUE` params.
fn params(raw: &[String]) -> Result<Vec<(String, String)>> {
    raw.iter()
        .map(|p| {
            p.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .ok_or_else(|| Error::msg(format!("--param {p}: expected KEY=VALUE")))
        })
        .collect()
}

/// `smllm fire` (CLI-4, HOST-5).
// @zen-impl: CLI-4_AC-1
pub fn fire(args: &FireArgs, explicit: Option<&Path>) -> Result<u8> {
    let ps = params(&args.params)?;
    let cwd = std::env::current_dir().map_err(|e| Error::io(Path::new("."), e))?;
    let mut rt = match &args.session {
        Some(k) => Runtime::for_session(k)?,
        None => Runtime::lookup(explicit, &cwd)?,
    };
    let cwd_s = cwd.display().to_string();
    let configs = rt.configs.clone();
    let bind = Bind {
        harness: "none",
        host_session: None,
        cwd: &cwd_s,
        configs: &configs,
    };
    let reply = rt.with(|e, h| e.fire(h, args.session.as_deref(), &args.event, &ps, &bind))?;
    if args.json {
        output::json(&reply)?;
    } else {
        output::text(&reply.text)?;
    }
    Ok(if reply.ok { EXIT_OK } else { EXIT_ERRORS })
}

/// `smllm session list`.
// @zen-impl: CLI-5_AC-1
pub fn session_list(args: &JsonArgs) -> Result<u8> {
    let store = FsStore::new(paths::user_state_dir()?, HashMap::new());
    let sessions = store.sessions().map_err(|e| Error::msg(e.to_string()))?;
    if args.json {
        return output::json(&json!({ "sessions": sessions })).map(|()| EXIT_OK);
    }
    let mut out = String::new();
    for s in &sessions {
        let at = match &s.holding {
            Some(k) => format!("{} {}", k.machine, k.id),
            None => "idle".to_string(),
        };
        out.push_str(&format!(
            "{}  {}  {}  last active {}  {}\n",
            s.key,
            s.harness,
            at,
            format_utc(s.last_active),
            s.cwd
        ));
    }
    if sessions.is_empty() {
        out.push_str("no sessions\n");
    }
    output::text(&out)?;
    Ok(EXIT_OK)
}

/// `smllm session show KEY`: the tool's no-event view.
pub fn session_show(args: &KeyArgs) -> Result<u8> {
    let mut rt = Runtime::for_session(&args.key)?;
    let reply = rt.with(|e, h| e.view(h, &args.key))?;
    if args.json {
        output::json(&reply)?;
    } else {
        output::text(&reply.text)?;
    }
    Ok(EXIT_OK)
}

/// `smllm instance list` (CLI-6).
// @zen-impl: CLI-6_AC-1
pub fn instance_list(args: &JsonArgs, explicit: Option<&Path>) -> Result<u8> {
    let cwd = std::env::current_dir().map_err(|e| Error::io(Path::new("."), e))?;
    let mut rt = Runtime::lookup(explicit, &cwd)?;
    let ids: Vec<String> = rt.loaded.machines.iter().map(|m| m.id.clone()).collect();
    let mut all = Vec::new();
    for id in ids {
        all.extend(
            rt.store
                .instances(&id)
                .map_err(|e| Error::msg(e.to_string()))?,
        );
    }
    if args.json {
        return output::json(&json!({ "instances": all })).map(|()| EXIT_OK);
    }
    let mut out = String::new();
    for i in &all {
        let held = i
            .holder
            .as_deref()
            .map(|h| format!("  held by {h}"))
            .unwrap_or_default();
        out.push_str(&format!(
            "{}  {} ({})  {}  {}{held}\n",
            i.machine,
            i.label(),
            i.id,
            i.state,
            i.status.as_str()
        ));
    }
    if all.is_empty() {
        out.push_str("no instances\n");
    }
    output::text(&out)?;
    Ok(EXIT_OK)
}

/// `smllm instance show ID`: by id or ref, with history.
pub fn instance_show(args: &KeyArgs, explicit: Option<&Path>) -> Result<u8> {
    let cwd = std::env::current_dir().map_err(|e| Error::io(Path::new("."), e))?;
    let mut rt = Runtime::lookup(explicit, &cwd)?;
    let ids: Vec<String> = rt.loaded.machines.iter().map(|m| m.id.clone()).collect();
    let mut found = None;
    for id in ids {
        for i in rt
            .store
            .instances(&id)
            .map_err(|e| Error::msg(e.to_string()))?
        {
            if i.id == args.key || i.r#ref.as_deref() == Some(&args.key) {
                found = Some(i);
            }
        }
    }
    let inst = found.ok_or_else(|| {
        Error::msg(format!(
            "no instance {} in the configured state machines",
            args.key
        ))
    })?;
    let history = rt
        .store
        .history(&inst.machine, &inst.id)
        .map_err(|e| Error::msg(e.to_string()))?;
    if args.json {
        return output::json(&json!({ "instance": inst, "history": history })).map(|()| EXIT_OK);
    }
    let mut out = format!(
        "{} {} ({})\nstate: {}  status: {}  holder: {}\nvisits: {}\nhistory:\n",
        inst.machine,
        inst.label(),
        inst.id,
        inst.state,
        inst.status.as_str(),
        inst.holder.as_deref().unwrap_or("-"),
        inst.visits
            .iter()
            .map(|(s, n)| format!("{s}={n}"))
            .collect::<Vec<_>>()
            .join(" "),
    );
    for h in &history {
        let route = match (&h.from, &h.to) {
            (Some(f), Some(t)) => format!("{f} → {t}"),
            (None, Some(t)) => format!("→ {t}"),
            (Some(f), None) => format!("{f} →"),
            (None, None) => String::new(),
        };
        out.push_str(&format!(
            "  {}  {}  {}  {route}\n",
            format_utc(h.at),
            h.session,
            h.event
        ));
        for (k, v) in h.params.iter() {
            out.push_str(&format!("      {k} = {v}\n"));
        }
        for t in &h.trace {
            out.push_str(&format!("      {t}\n"));
        }
    }
    output::text(&out)?;
    Ok(EXIT_OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_need_an_equals_sign() {
        assert_eq!(
            params(&["a=b=c".into()]).unwrap(),
            vec![("a".into(), "b=c".into())]
        );
        assert!(params(&["nope".into()]).is_err());
    }
}
