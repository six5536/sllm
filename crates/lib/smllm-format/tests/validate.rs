//! Loading, validation and lowering of machine files and configs (CFG, TEST-1).

use std::path::{Path, PathBuf};

use smllm_core::model::{ActionDef, GuardDef, Prompt};
use smllm_format::{ConfigFile, Level, Origin, compile, json_schema, load_configs, load_machine};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Findings as `line: level: path: message (rule)` without the file.
fn lines(path: &Path) -> Vec<String> {
    let (_, f) = load_machine(path, false);
    f.0.iter()
        .map(|f| {
            let r = f.render();
            let shown = path.display().to_string();
            r.strip_prefix(&shown).unwrap_or(&r).to_string()
        })
        .collect()
}

// @zen-test: CFG-1_AC-1
#[test]
fn the_examples_load_cleanly() {
    for ex in ["examples/showcase/config.toml", "examples/dev/config.toml"] {
        let loaded = load_configs(
            &[ConfigFile {
                path: root().join(ex),
                origin: Origin::Project,
            }],
            false,
        );
        let errs: Vec<String> = loaded
            .findings
            .0
            .iter()
            .filter(|f| f.level != Level::Info)
            .map(|f| f.render())
            .collect();
        assert!(errs.is_empty(), "{ex}: {errs:#?}");
        assert_eq!(loaded.config.machines.len(), 1);
        assert!(loaded.machines[0].state_dir.ends_with("state"));
    }
}

// @zen-test: CFG-9_AC-1
// @zen-test: CFG-11_AC-1
#[test]
fn the_showcase_lowers_as_written() {
    let (m, _) = load_machine(&root().join("examples/showcase/showcase.smllm.yaml"), false);
    let m = m.unwrap();
    assert_eq!(m.instance.kind, "document");
    assert_eq!(m.instance.ref_param, "documentPath");
    // setRef made the ref param required on `named`.
    let named = m.events.get("named").unwrap();
    assert!(named.param("documentPath").unwrap().required);
    assert!(named.param("documentPath").unwrap().pattern.is_some());
    let draft = m.state("DRAFT").unwrap();
    assert!(draft.entry_point);
    assert!(
        matches!(&draft.entry[0], ActionDef::Prompt(Prompt::File(f)) if f.ends_with("instructions/draft.md"))
    );
    assert!(draft.on("refine").unwrap().transitions[0].reenter);
    let check = m.state("CHECK").unwrap();
    assert_eq!(check.always.len(), 3);
    assert!(matches!(
        check.always[1].guard,
        Some(GuardDef::Visits { at_least: 5, .. })
    ));
    // No prompt in entry → the implied enter-<STATE>.md.
    let done = m.state("DONE").unwrap();
    assert!(done.is_final);
    assert_eq!(m.shared.len(), 2);
    assert!(m.fallback().is_some_and(|s| s.name == "DETOUR"));
    let stuck = m.state("STUCK").unwrap();
    assert!(
        !stuck
            .entry
            .iter()
            .any(|a| matches!(a, ActionDef::Prompt(Prompt::DefaultFile(_))))
    );
    let parked = m.state("PARKED").unwrap();
    assert!(parked.on.is_empty());
}

// @zen-test: CFG-13_AC-1
// @zen-test: CFG-14_AC-1
#[test]
fn every_rule_violation_is_collected() {
    let found = lines(&fixture("bad/rules.smllm.yaml"));
    insta::assert_snapshot!(found.join("\n"));
    let (m, _) = load_machine(&fixture("bad/rules.smllm.yaml"), false);
    assert!(m.is_none());
}

// @zen-test: CFG-2_AC-1
#[test]
fn unsupported_xstate_gets_a_hint() {
    let found = lines(&fixture("bad/xstate.smllm.yaml"));
    assert_eq!(found.len(), 1);
    assert!(
        found[0].starts_with(":6: error: states.A.after: `after` is not supported (CFG-2)"),
        "{found:?}"
    );
    assert!(found[0].contains("not in smllm v1's subset"), "{found:?}");
}

#[test]
fn version_and_missing_files() {
    let found = lines(&fixture("bad/version.smllm.yaml"));
    assert!(
        found
            .iter()
            .any(|l| l.contains("unsupported smllm format version 2")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|l| l.starts_with(":6: error: states.A.entry: prompt file nowhere.md")),
        "{found:?}"
    );
    let (_, f) = load_machine(&fixture("bad/absent.smllm.yaml"), false);
    assert!(f.0[0].message.starts_with("cannot read"));
}

// @zen-test: CFG-15_AC-2
#[test]
fn project_wins_over_user_and_idle_is_replaced() {
    let dir = std::env::temp_dir().join(format!("smllm-cfg-{}", std::process::id()));
    let (user, project) = (dir.join("user"), dir.join("project"));
    std::fs::create_dir_all(&user).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    let dev = std::fs::read_to_string(root().join("examples/dev/dev.smllm.yaml")).unwrap();
    for d in [&user, &project] {
        std::fs::write(d.join("dev.smllm.yaml"), &dev).unwrap();
        std::fs::write(d.join("triage.md"), "t").unwrap();
        std::fs::write(d.join("work.md"), "w").unwrap();
    }
    std::fs::write(
        user.join("config.toml"),
        "[machines]\nfiles = [\"dev.smllm.yaml\"]\n[idle]\non-enter = { text = \"user\" }\n",
    )
    .unwrap();
    std::fs::write(project.join("idle.md"), "project idle").unwrap();
    std::fs::write(
        project.join("config.toml"),
        "[machines]\nfiles = [\"dev.smllm.yaml\"]\n[idle]\non-enter = { file = \"idle.md\" }\n",
    )
    .unwrap();
    let loaded = load_configs(
        &[
            ConfigFile {
                path: user.join("config.toml"),
                origin: Origin::User,
            },
            ConfigFile {
                path: project.join("config.toml"),
                origin: Origin::Project,
            },
        ],
        false,
    );
    assert_eq!(loaded.config.machines.len(), 1);
    assert_eq!(
        loaded.source("dev").unwrap().config,
        project.join("config.toml")
    );
    assert!(
        loaded
            .findings
            .0
            .iter()
            .any(|f| f.level == Level::Info && f.message.contains("replaces"))
    );
    assert!(
        matches!(&loaded.config.idle[0], ActionDef::Prompt(Prompt::File(f)) if f.ends_with("idle.md"))
    );

    std::fs::write(project.join("config.toml"), "[machines]\nfile = []\n").unwrap();
    let bad = load_configs(
        &[ConfigFile {
            path: project.join("config.toml"),
            origin: Origin::Explicit,
        }],
        false,
    );
    assert!(bad.findings.has_errors());
    assert_eq!(bad.findings.0[0].line, Some(2));
    std::fs::write(project.join("config.toml"), "[idle]\non-enter = { }\n").unwrap();
    let bad = load_configs(
        &[ConfigFile {
            path: project.join("config.toml"),
            origin: Origin::Explicit,
        }],
        false,
    );
    assert!(
        bad.findings.0[0]
            .message
            .contains("exactly one of file, text")
    );
    std::fs::remove_dir_all(dir).ok();
}

// @zen-test: CLI-13_AC-1
#[test]
fn compile_inlines_prompts() {
    let (json, f) = compile(&root().join("examples/showcase/config.toml"));
    assert!(!f.has_errors());
    let json = json.unwrap();
    assert!(json.contains("Write the draft. State its goal"), "{json}");
    assert!(!json.contains("\"file\""), "no file prompts remain");
    let back: smllm_core::model::Config = serde_json::from_str(&json).unwrap();
    assert_eq!(back.machines[0].id, "showcase");
    let (json, _) = compile(&root().join("examples/dev/dev.smllm.yaml"));
    assert!(json.unwrap().contains("\"dev\""));
    let (json, f) = compile(&fixture("bad/rules.smllm.yaml"));
    assert!(json.is_none() && f.has_errors());
}

// @zen-test: CFG-15_AC-1
#[test]
fn the_json_schema_describes_the_format() {
    let s = json_schema();
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["title"], "smllm state machine");
    for key in ["id", "initial", "meta", "states"] {
        assert!(
            v["required"].as_array().unwrap().iter().any(|r| r == key),
            "{key}"
        );
    }
    assert!(s.contains("setRef") && s.contains("visits") && s.contains("entryPoint"));
}

// @zen-test: CFG-14_AC-1
// @zen-test: CFG-2_AC-1
#[test]
fn every_shape_problem_is_collected_at_its_line() {
    insta::assert_snapshot!(lines(&fixture("bad/shape.smllm.yaml")).join("\n"));
}

// @zen-test: CFG-13_AC-1
// @zen-test: CFG-9_AC-1
// @zen-test: CFG-7_AC-1
// @zen-test: CFG-12_AC-1
// @zen-test: CFG-8_AC-1
#[test]
fn lowering_checks_from_the_review() {
    let found = lines(&fixture("bad/review.smllm.yaml"));
    insta::assert_snapshot!(found.join("\n"));
    // One fence warning per text, not two.
    assert_eq!(
        found
            .iter()
            .filter(|l| l.contains("TURN-12") && l.contains("go.actions"))
            .count(),
        1,
        "{found:#?}"
    );
}

#[test]
fn set_ref_with_empty_params_and_duplicate_keys() {
    let dir = std::env::temp_dir().join(format!("smllm-misc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let ok = dir.join("ok.smllm.yaml");
    std::fs::write(&ok, "id: ok\ninitial: A\nmeta: {smllm: 1}\nstates:\n  A:\n    entry: {type: prompt, params: {text: hi}}\n    on:\n      named: {target: B, actions: {type: setRef, params: {}}}\n  B: {type: final}\n").unwrap();
    let (m, f) = load_machine(&ok, false);
    assert!(m.is_some(), "{f:?}");
    let dup = dir.join("dup.smllm.yaml");
    std::fs::write(
        &dup,
        "id: d\ninitial: A\ninitial: B\nmeta: {smllm: 1}\nstates: {A: {}}\n",
    )
    .unwrap();
    let found = lines(&dup);
    assert!(found[0].contains("duplicate key `initial`"), "{found:?}");
    assert!(!found[0].contains("DuplicateKeyPolicy"));
    std::fs::remove_dir_all(dir).ok();
}
