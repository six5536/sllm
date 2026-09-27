# PLAN-004: Faster hooks, less duplication

| Meta               | Value                                                        |
| ------------------ | ------------------------------------------------------------ |
| Status             | in-progress (draft)                                          |
| Workflow direction | bottom-up (review findings → code → specs touched per phase) |
| Traces to          | NFR-2, NFR-8, TURN-4..8, CFG-4, CFG-7, STO-2, STO-3, DEC-6, INST-3, INST-9 |

## 1. Goal

Act on the 2026-09-27 DRY and performance review. Every hook, status line refresh and MCP call
starts `smllm` and loads the whole config: `smllm instance list` takes about 12 ms on this repo,
against 0.5 ms for `--version`. NFR-2_AC-1 records 1–2 ms. Most of that time is checking work
that only `validate` reports. Bring the hook paths back to a few ms and remove the duplication
the review found. Behaviour is unchanged unless an item says otherwise. Out of scope: the
cross-run config cache (not worth it after P1–P2).

## 2. Findings (requirements)

Performance (`P-` ids):

| ID  | Finding (file) → fix | Traces |
| --- | -------------------- | ------ |
| P-1 | Each param `pattern` is fully compiled on every load, just to check it is valid; if enum values are present it is compiled twice (`lower/events.rs:16`, `:180`). About 8 of the 12 ms (reviewer's measurement, to be confirmed in phase A1) → cheaper check (Q1); compile once when enum values are present | NFR-2, CFG-7 |
| P-2 | Every prompt file, and every `enter-<STATE>.md` probe, is read on every load, just to check fences (`lower/actions.rs:110`, `:155`); only `validate` prints that warning → run mode checks `is_file()` only (a missing file is still an error); `default_prompt` does not touch the disk | NFR-2, CFG-4 |
| P-3 | user-prompt-submit loads every machine; `prompt_submitted` needs only the session (`harness.rs:343`) → no config load | NFR-2, TURN-8 |
| P-4 | stop loads every machine before `Engine::stop` allows at once for idle or yielded sessions (`harness.rs:350`) → decide from the session first (one core rule, shared by the engine and the CLI) | NFR-2, TURN-4..6 |
| P-5 | The session is read two or three times per hook (`bound_key`, `for_session`, the session-start check, then the engine) → `for_session` returns the session it read | NFR-2, STO-2 |
| P-6 | stop reads and parses the held instance twice (`api.rs:299-300`: `held` then `view` → `held`) → `view` takes the pair `held` returned | NFR-2, TURN-4 |
| P-7 | The command runner polls the readers with a 5 ms sleep after the child exits; most commands pay it (`host.rs:147-150`) → each reader signals on a channel; `recv_timeout` against the same 200 ms deadline | DEC-6 |
| P-8 | Every instance write scans and parses the whole instance directory (`store.rs:228`, `ref_taken`) → check the ref only when it differs from the stored instance's | STO-3, INST-3 |
| P-9 | `Runtime::new` deep-copies the whole config (`runtime.rs:38`) → move it into the engine; readers use `engine.config()` | — |
| P-10 | `config.toml` is parsed twice per lookup (`paths::configures`, then `load_configs`) → `load_configs` skips a file with neither `[machines]` nor `[idle]`, and the rule moves beside `ConfigToml` | STO-2 |
| P-11 | Parked-list sort builds two Strings per comparison (`idle.rs:148`) → `insertion_sort_by` with a comparator, replacing `_by_key` | — |
| P-12 | `validate` scans each machine directory twice (`config.rs:156-174`, `unreadable` + `instances`) → one scan | — |

DRY (`D-` ids, behaviour unchanged):

| ID  | Finding → fix |
| --- | ------------- |
| D-1 | The "held by this session with this status" check is written about six times (`idle.rs:116-130`, `:449-458`, `machine.rs:84`, `:426`, `status.rs:83-92`) → `Instance::held_by(key, status)` plus one lookup shared by `status.rs` and `idle::list` |
| D-2 | `Turn::enter` / `Turn::exit` are near-copies (`turn.rs:230-275`) → private `run_lists` |
| D-3 | `Engine::view` / `menu` differ by one flag (`api.rs:219-237`) → private `show` |
| D-4 | "names joined by commas" rebuilt through a temporary `Vec` about 12 times → `push_joined` plus `Machine::state_names()` / `entry_point_names()`; one wording for "no entry points" |
| D-5 | Parsing "params is a JSON object of strings" appears twice, with the same error text (`mcp.rs:81-93`, `smllm-wasm lib.rs:256-267`) → one core `params_from_json` |
| D-6 | TOML "parse or refuse" written four times (kit `record.rs:141`, `declined.rs:274`, `:339`, app `config.rs:117`) → kit `parse_toml` beside `parse_json` |
| D-7 | The "event sets the ref" scan appears three times, once inside a per-param loop (`lower/machine.rs:146`, `:195`, `:291`) → `sets_ref` helper, hoisted; CFG-13 check uses `set_ref_events` |
| D-8 | Two identical `MachineSource` constructions (`load.rs:311-325`) → build once |
| D-9 | `FsStore::new(user_state_dir, empty)` three times → `FsStore::user()`; instance lookup across machines twice in `state.rs` → `Runtime::all_instances()`; `instance show` tries the id directly first; "JSON or text" output twice → `output::reply` |
| D-10 | Tests: MCP JSON-RPC client written three times, `_meta` block twice (`tests/session.rs`), `statusline.rs` re-implements `start` → `tests/common` helpers; per-crate temp-dir copies → one helper per crate (no `tempfile` dependency) |

## 3. Design sketch

- LOAD MODE: `smllm-format` replaces `inline: bool` with
  `pub enum Mode { Run, Check, Inline }`. `Files { dir, mode }`.
  `Run` (hooks, MCP, status line, the other commands) does only what the engine needs.
  `Check` (`validate`) and `Inline` (`compile`) keep today's full checks and findings.
  `load_configs(files, Mode)`, `load_machine(path, Mode)`. `Runtime::new(files)` uses `Run`;
  `Runtime::checked(files)` uses `Check`, for `validate`. Findings that only `Check` computes are
  warnings, so run mode's error set stays the same, pattern errors aside (Q1).
- STOP AND PROMPT HOOKS: core `Session::may_stop(&self) -> bool` (`yielded || holding.is_none()`),
  used by `Engine::stop` and by the CLI before loading. `Runtime::for_session(key) -> (Runtime, Option<Session>)`,
  or a bare `Runtime::session_only()` over `FsStore::user()` for `prompt_submitted`.
- ENGINE: `machine::view_held(turn, machine, &inst, entry)`; `view` = `held` + `view_held`;
  `stop` calls `view_held`. `utils::sort::insertion_sort_by(&mut [T], impl FnMut(&T,&T)->Ordering)`.
- RUNNER: `std::sync::mpsc` channel; each reader thread sends `()` on exit.
- STORE: `put_instance` keeps the stored `Instance`; `ref_taken` only when
  `instance.r#ref != stored.r#ref`.
- Wasm budget: 300 KiB (now 259 KiB); D-4/D-5 should shrink it slightly. Check after P1 and P5.

## 4. Phases

Each phase is one commit and passes `npm run -s test:gate`. Timing is `for i in $(seq 50); do smllm instance list; done`
with the release binary, recorded before A1 and after A2.

| Phase | Name | Items | Proof |
| ----- | ---- | ----- | ----- |
| A1 | Load mode | P-1, P-2, P-10 | `validate` snapshots unchanged; a run-mode test with a missing prompt file still errors; timing |
| A2 | Hooks read less | P-3, P-4, P-5, P-9, D-9 (store helper) | session tests unchanged; a stop on an idle session with a broken config allows (proves no load); timing |
| A3 | Engine | P-6, P-11, D-1, D-2, D-3, D-4 | engine and instance tests unchanged; wasm size |
| A4 | Runner and store | P-7, P-8, P-12 | host tests; a test that a command's output is complete without the sleep; ref-clash test still fails a duplicate |
| A5 | Shared helpers | D-5, D-6, D-7, D-8, D-9 (rest) | MCP and wasm smoke tests; harness tests |
| A6 | Test helpers | D-10 | test count unchanged |
| A7 | Docs and outcome | NFR-2_AC-1 measurement, DESIGN-CFG (modes), DESIGN-DEC (runner), ARCHITECTURE change log, this plan's §7 | — |

## 5. Open questions

- Q1: How to make the pattern check cheap?
  - (a) Recommended: add `regex-syntax` as a direct dependency of `smllm-format` and use its
    parser for the syntax check. It is already in the tree through `regex` (same version), so it
    adds no code. It needs your approval.
  - (b) Run mode skips the pattern check. A bad pattern then fails at fire time, with a
    rejection from the host matcher, instead of dropping the machine at load. `validate` still reports it.
- Q2: Instance scans grow with history (INST-9 keeps completed instances forever). The status
  line, idle view and ref checks parse every file. Fix now by moving completed instances to
  `<machine>/done/`, which changes the store layout and needs a migration or a fallback read,
  or defer? Recommended: defer, since it costs nothing at today's sizes.
- Q3: Rust's `\d` matches any Unicode digit; JavaScript's (wasm host) matches only 0–9, so the
  two hosts can disagree on one pattern. Leave as is (recommended; document it in CFG-7), or
  make the CLI match ASCII-only (`RegexBuilder::unicode(false)`, which rejects some patterns)?

## 6. Decisions

(filled in at GRILL)

## 7. Outcome

(filled in at the end)
