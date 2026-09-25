// Derived from sokf 9c93f37 crates/lib/sokf-core/src/api/harness.rs
//! `<tool> harness install` and `status`, generic over a [`Tool`].

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{
    Error, Result,
    harness::{
        Content, Markers, Observed, Part, Plan, Profile, Record, Scope, State, Tool, apply_plan,
        expected, hash, observe, read_record, render_files, render_merge, render_record,
        render_region, state, target_path,
    },
};

/// The options of `install`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstallOptions {
    /// The harness (profile) name, e.g. `claude`.
    pub harness: String,
    /// `--scope`.
    pub scope: Scope,
    /// `--without`, when given at all: replaces the declined parts.
    pub without: Option<Vec<String>>,
    /// `--force`: write a part whose state is `edited`.
    pub force: bool,
}

/// One line of the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PartResult {
    /// The part's name.
    pub part: String,
    /// The verb of `install` (`created`, `rewrote`, `updated`, `current`,
    /// `edited`, `skipped`) or the state of `status`.
    pub state: String,
    /// The part's path relative to the root, `/`-separated; an external
    /// part's location.
    pub path: String,
}

/// The outcome of `install` or `status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HarnessResult {
    /// The harness name.
    pub harness: String,
    /// The scope.
    pub scope: Scope,
    /// The root directory the paths are relative to.
    pub root: String,
    /// One entry per part, in profile order.
    pub parts: Vec<PartResult>,
}

impl HarnessResult {
    /// The text report: one line per part, `<verb> <path> (<part>)`, the
    /// verb padded to seven columns.
    pub fn to_text(&self) -> String {
        self.parts
            .iter()
            .map(|p| format!("{:<7} {} ({})\n", p.state, p.path, p.part))
            .collect()
    }
}

/// A part's state and what the tree held, computed before any write.
struct Examined<'a> {
    part: &'a Part,
    path: String,
    state: State,
    existed: bool,
}

/// Everything `install` and `status` read.
struct Context {
    profile: Profile,
    root: PathBuf,
    markers: Markers,
    record_path: PathBuf,
    record: Record,
}

/// `path` relative to `root` when it lies under it, `/`-separated.
fn display(root: &Path, path: &Path) -> String {
    let shown = path.strip_prefix(root).unwrap_or(path);
    shown.to_string_lossy().replace('\\', "/")
}

fn context<T: Tool + ?Sized>(tool: &T, harness: &str, scope: Scope) -> Result<Context> {
    let profile = tool
        .profile(harness, scope)
        .ok_or_else(|| Error::Harness(format!("no profile named `{harness}`")))?;
    let root = tool.root(scope)?;
    let record_path = tool.record_path(scope)?;
    let record = read_record(&record_path, &display(&root, &record_path))?;
    Ok(Context {
        profile,
        markers: Markers::new(tool.name()),
        root,
        record_path,
        record,
    })
}

/// Examine every part: its target, what the tree holds, its state.
fn examine<'a>(cx: &'a Context, declined: &[String]) -> Result<Vec<Examined<'a>>> {
    let recorded = cx.record.harnesses.get(&cx.profile.harness);
    let mut out = Vec::new();
    for part in &cx.profile.parts {
        let path = target_path(&cx.root, part)?;
        let observed = observe(&cx.root, part, &path, &cx.markers)?;
        let hash = recorded.and_then(|m| m.get(&part.name)).map(String::as_str);
        let state = state(
            &observed,
            &expected(part),
            hash,
            declined.contains(&part.name),
        );
        let present = matches!(observed, Observed::Present(_));
        let existed = present
            || (!matches!(part.content, Content::External(_)) && cx.root.join(&path).exists());
        out.push(Examined {
            part,
            path,
            state,
            existed,
        });
    }
    Ok(out)
}

/// The existing text of `path`: planned already, or on disk.
fn existing_text(plan: &Plan, path: &Path) -> Result<Option<String>> {
    if let Some(text) = plan.pending(path) {
        return Ok(Some(text.to_string()));
    }
    crate::harness::read_text(path)
}

/// Plan the write of one part, from the pending or the on-disk text.
fn plan_part(cx: &Context, plan: &mut Plan, e: &Examined<'_>) -> Result<()> {
    let fs_path = cx.root.join(&e.path);
    match &e.part.content {
        Content::Files(_) => {
            for (rel, text) in render_files(e.part) {
                plan.set(fs_path.join(rel), text);
            }
        }
        Content::Block(block) => {
            let existing = existing_text(plan, &fs_path)?;
            if let Some(text) = render_region(existing.as_deref(), block, &cx.markers) {
                plan.set(fs_path, text);
            }
        }
        Content::Merge(ops) => {
            let existing = existing_text(plan, &fs_path)?;
            if let Some(text) = render_merge(existing.as_deref(), ops, &e.path)? {
                plan.set(fs_path, text);
            }
        }
        Content::External(ext) => plan.externals.push(ext.clone()),
    }
    Ok(())
}

/// Make the tree match the profile: a declined part is skipped, an absent
/// part created, a stale part rewritten, an edited part left unless
/// `force`, a current part left. Every refusal comes before any write.
pub fn install<T: Tool + ?Sized>(tool: &T, opts: &InstallOptions) -> Result<HarnessResult> {
    let cx = context(tool, &opts.harness, opts.scope)?;
    if let Some(part) = opts
        .without
        .iter()
        .flatten()
        .find(|p| cx.profile.part(p).is_none())
    {
        return Err(Error::Harness(format!(
            "profile `{}` has no part named `{part}`",
            cx.profile.harness
        )));
    }
    let store = tool.declined_store(opts.scope)?;
    let stored = store.declined(&cx.profile.harness)?;
    let declined = opts.without.clone().unwrap_or(stored);
    let examined = examine(&cx, &declined)?;

    let mut plan = Plan::default();
    let mut recorded = cx
        .record
        .harnesses
        .get(&cx.profile.harness)
        .cloned()
        .unwrap_or_default();
    let mut parts = Vec::new();
    for e in &examined {
        let written = match e.state {
            State::Skipped | State::Current => false,
            State::Absent | State::Stale => true,
            State::Edited => opts.force,
        };
        let verb = match e.state {
            State::Skipped => "skipped",
            State::Current => "current",
            State::Edited if !written => "edited",
            _ if !e.existed => "created",
            _ if matches!(e.part.content, Content::Files(_)) => "rewrote",
            _ => "updated",
        };
        if written {
            plan_part(&cx, &mut plan, e)?;
        }
        match e.state {
            State::Skipped => {
                recorded.remove(&e.part.name);
            }
            State::Edited if !written => {}
            _ => {
                recorded.insert(e.part.name.clone(), hash(&expected(e.part)));
            }
        }
        parts.push(PartResult {
            part: e.part.name.clone(),
            state: verb.to_string(),
            path: e.path.clone(),
        });
    }
    let mut new_record = cx.record.clone();
    new_record
        .harnesses
        .insert(cx.profile.harness.clone(), recorded);
    if new_record != cx.record || !cx.record_path.is_file() {
        plan.record = Some((
            cx.record_path.clone(),
            render_record(&new_record, &tool.record_header()),
        ));
    }
    apply_plan(&plan)?;
    if let Some(without) = &opts.without {
        store.set_declined(&cx.profile.harness, without)?;
    }
    Ok(HarnessResult {
        harness: cx.profile.harness.clone(),
        scope: opts.scope,
        root: cx.root.display().to_string(),
        parts,
    })
}

/// The state of every part; nothing is written.
pub fn status<T: Tool + ?Sized>(tool: &T, harness: &str, scope: Scope) -> Result<HarnessResult> {
    let cx = context(tool, harness, scope)?;
    let declined = tool.declined_store(scope)?.declined(&cx.profile.harness)?;
    let parts = examine(&cx, &declined)?
        .into_iter()
        .map(|e| PartResult {
            part: e.part.name.clone(),
            state: e.state.as_str().to_string(),
            path: e.path,
        })
        .collect();
    Ok(HarnessResult {
        harness: cx.profile.harness.clone(),
        scope,
        root: cx.root.display().to_string(),
        parts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_relative_under_the_root() {
        assert_eq!(
            display(Path::new("/r"), Path::new("/r/.tool/h.toml")),
            ".tool/h.toml"
        );
        assert_eq!(
            display(Path::new("/r"), Path::new("/x/h.toml")),
            "/x/h.toml"
        );
    }

    #[test]
    fn text_pads_the_verb() {
        let r = HarnessResult {
            harness: "claude".into(),
            scope: Scope::Project,
            root: "/r".into(),
            parts: vec![
                PartResult {
                    part: "hooks".into(),
                    state: "created".into(),
                    path: ".claude/settings.json".into(),
                },
                PartResult {
                    part: "instructions".into(),
                    state: "edited".into(),
                    path: "CLAUDE.md".into(),
                },
            ],
        };
        assert_eq!(
            r.to_text(),
            "created .claude/settings.json (hooks)\nedited  CLAUDE.md (instructions)\n"
        );
    }
}
