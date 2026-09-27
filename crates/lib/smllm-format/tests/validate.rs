//! Loading, validation and lowering of machine files and configs (CFG, TEST-1).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use smllm_core::model::{ActionDef, GuardDef, Prompt};
use smllm_format::{
    ConfigFile, Mode, Origin, Severity, compile, json_schema, load_configs, load_machine,
};

/// A fresh, empty temporary directory, unique per call and process.
fn temp_dir(name: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("smllm-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Findings as `validate` prints them, the file shown as `f`.
fn lines(path: &Path) -> Vec<String> {
    let (_, f) = load_machine(path, Mode::Check);
    f.0.iter().map(|f| f.to_report("f").to_line()).collect()
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
            Mode::Check,
        );
        let errs: Vec<String> = loaded
            .findings
            .0
            .iter()
            .filter(|f| f.level != Severity::Info)
            .map(|f| f.to_report("f").to_line())
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
    let (m, _) = load_machine(
        &root().join("examples/showcase/showcase.smllm.yaml"),
        Mode::Check,
    );
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
    let (m, _) = load_machine(&fixture("bad/rules.smllm.yaml"), Mode::Check);
    assert!(m.is_none());
}

// @zen-test: CFG-2_AC-1
#[test]
fn unsupported_xstate_gets_a_hint() {
    let found = lines(&fixture("bad/xstate.smllm.yaml"));
    assert_eq!(found.len(), 1);
    assert!(
        found[0].starts_with("f:6: error: states.A.after: `after` is not supported — XState"),
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
            .any(|l| l.starts_with("f:6: error: states.A.entry: prompt file nowhere.md")),
        "{found:?}"
    );
    let (_, f) = load_machine(&fixture("bad/absent.smllm.yaml"), Mode::Check);
    assert!(f.0[0].message.starts_with("cannot read"));
}

// @zen-test: CFG-15_AC-2
#[test]
fn project_wins_over_user_and_idle_is_replaced() {
    let dir = temp_dir("cfg");
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
        Mode::Check,
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
            .any(|f| f.level == Severity::Info && f.message.contains("replaces"))
    );
    assert!(
        matches!(&loaded.config.idle[0], ActionDef::Prompt(Prompt::File(f)) if f.ends_with("idle.md"))
    );

    // A broken override leaves the id unconfigured: the user's machine is not
    // used in its place (PLAN-003 F8).
    let both = [
        ConfigFile {
            path: user.join("config.toml"),
            origin: Origin::User,
        },
        ConfigFile {
            path: project.join("config.toml"),
            origin: Origin::Project,
        },
    ];
    std::fs::write(
        project.join("dev.smllm.yaml"),
        format!("{dev}\n  : [broken\n"),
    )
    .unwrap();
    let broken = load_configs(&both, Mode::Check);
    assert!(broken.config.machines.is_empty(), "{:?}", broken.machines);
    assert!(
        broken
            .findings
            .0
            .iter()
            .any(|f| f.level == Severity::Warning
                && f.message.contains("is not used while this file"))
    );
    std::fs::write(project.join("dev.smllm.yaml"), &dev).unwrap();
    // Ids that differ only in case cannot share one config (PLAN-003 F27).
    std::fs::write(
        project.join("dev2.smllm.yaml"),
        dev.replace("id: dev", "id: Dev"),
    )
    .unwrap();
    std::fs::write(
        project.join("config.toml"),
        "[machines]\nfiles = [\"dev.smllm.yaml\", \"dev2.smllm.yaml\"]\n",
    )
    .unwrap();
    let clash = load_configs(&both, Mode::Check);
    assert!(
        clash
            .findings
            .0
            .iter()
            .any(|f| f.level == Severity::Error
                && f.message.contains("differs from dev only in case"))
    );
    assert_eq!(clash.config.machines.len(), 1);
    std::fs::write(project.join("config.toml"), "[machines\n").unwrap();
    let broken = load_configs(&both, Mode::Check);
    assert!(broken.config.machines.is_empty());
    assert!(
        broken
            .findings
            .0
            .iter()
            .any(|f| f.message.contains("not loaded until this file is fixed"))
    );

    std::fs::write(project.join("config.toml"), "[machines]\nfile = []\n").unwrap();
    let bad = load_configs(
        &[ConfigFile {
            path: project.join("config.toml"),
            origin: Origin::Explicit,
        }],
        Mode::Check,
    );
    assert!(bad.findings.has_errors());
    assert_eq!(bad.findings.0[0].line, Some(2));
    std::fs::write(project.join("config.toml"), "[idle]\non-enter = { }\n").unwrap();
    let bad = load_configs(
        &[ConfigFile {
            path: project.join("config.toml"),
            origin: Origin::Explicit,
        }],
        Mode::Check,
    );
    assert!(
        bad.findings.0[0]
            .message
            .contains("exactly one of text, file")
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
    let dir = temp_dir("misc");
    let ok = dir.join("ok.smllm.yaml");
    std::fs::write(&ok, "id: ok\ninitial: A\nmeta: {smllm: 1}\nstates:\n  A:\n    entry: {type: prompt, params: {text: hi}}\n    on:\n      named: {target: B, actions: {type: setRef, params: {}}}\n  B: {type: final}\n").unwrap();
    let (m, f) = load_machine(&ok, Mode::Check);
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

// Null values count as absent, as in the JSON Schema; a prompt without
// text or file is reported at its line (PLAN-003 F26).
// @zen-test: CFG-14_AC-1
#[test]
fn nulls_are_absent_and_an_empty_prompt_has_a_line() {
    let dir = temp_dir("nulls");
    let file = dir.join("tiny.smllm.yaml");
    let tiny = "id: tiny\ndescription: ~\ninitial: A\nmeta:\n  smllm: 1\n  instance:\n  events: ~\nstates:\n  A:\n    description:\n    entry: ~\n    on:\n      go: B\n  B:\n    type: final\n";
    std::fs::write(&file, tiny).unwrap();
    let (m, f) = load_machine(&file, Mode::Check);
    assert!(!f.has_errors(), "{:?}", f.0);
    assert_eq!(m.unwrap().id, "tiny");
    let bad = tiny.replace("    entry: ~\n", "    entry:\n      - type: prompt\n");
    std::fs::write(&file, bad).unwrap();
    let (m, f) = load_machine(&file, Mode::Check);
    assert!(m.is_none());
    let e =
        f.0.iter()
            .find(|f| f.message.contains("exactly one of: text, file"))
            .unwrap();
    assert_eq!(e.line, Some(11), "{e:?}");
    std::fs::remove_dir_all(dir).ok();
}

// A run load checks that each named prompt file exists but reads none, and
// probes no `enter-<STATE>.md`: the fence warnings are `validate`'s
// (PLAN-004 D4-4).
// @zen-test: CFG-4_AC-1
#[test]
fn a_run_load_checks_prompt_files_without_reading_them() {
    let dir = temp_dir("run");
    std::fs::create_dir_all(dir.join("dir.md")).unwrap();
    std::fs::write(dir.join("p.md"), "say </smllm>").unwrap();
    std::fs::write(dir.join("enter-B.md"), "say </events>").unwrap();
    let file = dir.join("m.smllm.yaml");
    let machine = |prompt: &str| {
        format!(
            "id: m\ninitial: A\nmeta: {{smllm: 1}}\nstates:\n  A:\n    entry: {{type: prompt, params: {{file: {prompt}}}}}\n    on: {{go: B}}\n  B:\n    entry: {{type: prompt, params: {{file: p.md}}}}\n    on: {{go: C}}\n  C: {{type: final}}\n"
        )
    };
    std::fs::write(&file, machine("p.md")).unwrap();
    let fences = |f: &smllm_format::Findings| f.0.iter().filter(|f| f.rule == "TURN-12").count();

    let (m, f) = load_machine(&file, Mode::Check);
    assert!(m.is_some(), "{:?}", f.0);
    assert_eq!(fences(&f), 2, "{:?}", f.0);

    let (m, f) = load_machine(&file, Mode::Run);
    assert_eq!(fences(&f), 0, "{:?}", f.0);
    let m = m.unwrap();
    let a = m.state("A").unwrap();
    let p = dir.join("p.md").display().to_string();
    assert!(matches!(&a.entry[..], [ActionDef::Prompt(Prompt::File(f))] if *f == p));
    let c = m.state("C").unwrap();
    assert!(matches!(
        &c.entry[..],
        [ActionDef::Prompt(Prompt::DefaultFile(_))]
    ));

    for missing in ["nowhere.md", "dir.md"] {
        std::fs::write(&file, machine(missing)).unwrap();
        let (m, f) = load_machine(&file, Mode::Run);
        assert!(m.is_none(), "{missing}");
        assert!(
            f.0.iter()
                .any(|f| f.message.starts_with(&format!("prompt file {missing}"))),
            "{:?}",
            f.0
        );
    }
    std::fs::remove_dir_all(dir).ok();
}
