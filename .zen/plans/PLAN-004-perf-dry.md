# PLAN-004: Faster hooks, less duplication

| Meta               | Value                                                        |
| ------------------ | ------------------------------------------------------------ |
| Status             | completed (A1–A7, 2026-09-27)                                 |
| Workflow direction | bottom-up (review findings → code → specs touched per phase) |
| Traces to          | NFR-2, NFR-8, TURN-4..8, CFG-4, CFG-7, STO-2, STO-3, DEC-6, INST-3, INST-9 |

## 1. Goal

Act on the 2026-09-27 DRY and performance review. Every hook, status line refresh and MCP call
starts `smllm` and loads the whole config. NFR-2_AC-1 records 1–2 ms. Measured 2026-09-27 on this
repo's `.smllm` (4 machines), 50-run averages of `smllm graph --config …`:

| Where the config lives | Time | Notes |
| ---------------------- | ---- | ----- |
| `/workspaces` (virtiofs, a devcontainer on a Mac) | 11.0 ms | 48 file opens at about 0.13 ms each |
| local disk (`target/`, ext4) | 4.8 ms | CPU: YAML, shape check, lowering |
| local disk, no `pattern:` lines | 4.0 ms | pattern compiling costs about 0.8 ms |
| no config (`session list`) | 0.7 ms | process start |

The load makes 48 opens: 5 config and machine files, 11 prompt files in 29 opens (two of them
read 10 times each, once per state that names them), and 14 probes for `enter-<STATE>.md` files that
don't exist. Only the 5 are needed to run. The review's claim that patterns cost 8 of 12 ms was
wrong: the cost is file access on a slow mount, plus about 2.5 ms of load CPU not yet attributed.

Goal: hooks back near NFR-2's figure on both disks, and the duplication the review found removed.
Behaviour is unchanged unless an item says otherwise. Out of scope: the cross-run config cache
(revisit only if A1 leaves the local-disk load above about 2 ms).

## 2. Findings (requirements)

Performance (`P-` ids):

| ID  | Finding (file) → fix | Traces |
| --- | -------------------- | ------ |
| P-1 | Each param `pattern` is fully compiled on every load, just to check it is valid, and compiled again when enum values are present (`lower/events.rs:16`, `:180`); about 0.8 ms → keep the check (D4-1); compile once, reusing it for the enum check | NFR-2, CFG-7 |
| P-2 | Every prompt file is read, once per state that names it, and every `enter-<STATE>.md` is probed, on every load, only for the fence warning that `validate` prints (`lower/actions.rs:110`, `:155`); 43 of 48 file opens → run mode: no reads and no probes; each named file is checked for existence once per load, not once per mention (D4-4) | NFR-2, CFG-4 |
| P-3 | user-prompt-submit loads every machine; `prompt_submitted` needs only the session (`harness.rs:343`) → no config load | NFR-2, TURN-8 |
| P-4 | stop loads every machine before `Engine::stop` allows at once for idle or yielded sessions (`harness.rs:350`) → decide from the session first (one core rule, shared by the engine and the CLI) | NFR-2, TURN-4..6 |
| P-5 | The session is read two or three times per hook (`for_session`, the session-start check, then the engine) → `for_session` returns the session it read | NFR-2, STO-2 |
| P-6 | stop reads and parses the held instance twice (`api.rs:299-300`: `held` then `view` → `held`) → `view` takes the pair `held` returned | NFR-2, TURN-4 |
| P-7 | The command runner polls the readers with a 5 ms sleep after the child exits; most commands pay it (`host.rs:147-150`) → each reader signals on a channel; `recv_timeout` against the same 200 ms deadline | DEC-6 |
| P-8 | Every instance write scans and parses the whole instance directory (`store.rs:228`, `ref_taken`) → check the ref only when it differs from the stored instance's (a new instance, or `setRef`), still under the lock, so PLAN-003 F4 holds | STO-3, INST-3 |
| P-9 | `Runtime::new` deep-copies the whole config (`runtime.rs:39`) → move it into the engine; `graph.rs:136` and `config.rs:170` use `engine.config()` | — |
| P-10 | Load CPU on local disk (about 2.5 ms besides patterns and process start) is not attributed; the suspect is the shape check building about 13 JSON Schemas per process (`shape.rs:28-55`) → time each load stage in A1 and fix the largest (one shared `schema_for!(MachineFile)` if it is the schemas) | NFR-2 |
| P-11 | Parked-list sort builds two Strings per comparison (`idle.rs:148`) → `insertion_sort_by` with a comparator, replacing `_by_key` (crate-private) | — |
| P-12 | `validate` scans each machine directory twice (`config.rs:156-174`, `unreadable` + `instances`) → one scan | — |

DRY (`D-` ids, behaviour unchanged unless noted):

| ID  | Finding → fix |
| --- | ------------- |
| D-1 | The "held by this session with this status" check is written about six times (`idle.rs:116-130`, `:449-458`, `machine.rs:84`, `:426`, `status.rs:83-92`) → `Instance::held_by(key, status)` plus one lookup shared by `status.rs` and `idle::list` |
| D-2 | `Turn::enter` / `Turn::exit` are near-copies (`turn.rs:230-275`) → private `run_lists` |
| D-3 | `Engine::view` / `menu` differ by one flag (`api.rs:219-237`) → private `show` |
| D-4 | "names joined by commas" rebuilt through a temporary `Vec` about 12 times → `push_joined` plus `Machine::state_names()` / `entry_point_names()`. The two "no entry points" texts (`idle.rs:284-288` `(none: no state is an entry point)`, `:387-391` `none`) become one; this changes output, and the snapshots with it |
| D-5 | "params is an object of strings" parsed twice with the same error text (`mcp.rs:81-93`, `smllm-wasm lib.rs:256-267`). Core has no `serde_json` (dev-dependency only) → core `params_from(impl Iterator<Item = (&str, Option<&str>)>) -> Result<Vec<(String, String)>, Error>` holds the rule and the text; each caller maps its JSON values to `Option<&str>` |
| D-6 | TOML "parse or refuse" written four times (kit `record.rs:141`, `declined.rs:274`, `:339`, app `config.rs:117`) → kit `parse_toml` beside `parse_json` (the kit already depends on `toml_edit`) |
| D-7 | The "event sets the ref" scan appears three times, once inside a per-param loop (`lower/machine.rs:146`, `:195`, `:291`) → `sets_ref` helper, hoisted; CFG-13 check uses `set_ref_events` |
| D-8 | Two identical `MachineSource` constructions (`load.rs:311-325`) → build once |
| D-9 | `FsStore::new(user_state_dir, empty)` three times → `FsStore::user()`; instance lookup across machines twice in `state.rs` → `Runtime::all_instances()`; `instance show` tries the id directly first; "JSON or text" output twice → `output::reply` |
| D-10 | Tests: MCP JSON-RPC client written three times, `_meta` block twice (`tests/session.rs`), `statusline.rs` re-implements `start` → `tests/common` helpers. Temp dirs: about 9 copies → per crate one `#[cfg(test)]` helper for unit tests in `src/` and one in `tests/common`; no `tempfile` dependency |

Dropped at double-check: moving the "configures smllm" rule (`paths::configures`) into
`load_configs`. `lookup` uses it to choose which files count, and `init` uses it, so moving it
would change which configs a session binds to. The saved second parse is microseconds.

## 3. Design sketch

- LOAD MODE: `smllm-format` replaces `inline: bool` with `pub enum Mode { Run, Check, Inline }`;
  `Files { dir, mode }`.
  - `Run`: hooks, MCP, the status line and the other commands, through `Runtime::new`. Does only
    what the engine needs (P-1, P-2).
  - `Check`: `validate`, through `Runtime::checked`. Today's full checks and warnings.
  - `Inline`: `compile`. Full checks, prompt texts copied into the JSON.
  - Signatures: `load_configs(files, Mode)`, `load_machine(path, Mode)`.
  - Load warnings are printed only by `validate`, so `Run` skipping warning-only work is invisible
    elsewhere. A missing named prompt file is still an error in `Run`.
- STOP AND PROMPT HOOKS: core `Session::may_stop(&self) -> bool` (`yielded || holding.is_none()`),
  used by `Engine::stop` and by the CLI before loading. `Runtime::for_session(key) -> (Runtime, Option<Session>)`;
  `prompt_submitted` runs over a bare `FsStore::user()`.
- ENGINE: `machine::view_held(turn, machine, &inst, entry)`; `view` = `held` + `view_held`;
  `stop` calls `view_held`. `utils::sort::insertion_sort_by(&mut [T], impl FnMut(&T, &T) -> Ordering)`.
- RUNNER: `std::sync::mpsc`; each reader thread sends `()` when it ends.
- STORE: `put_instance` keeps the stored `Instance`; `ref_taken` only when
  `instance.r#ref != stored.r#ref`.
- Wasm budget: 300 KiB (now 259 KiB); smllm-wasm does not use smllm-format, so only A3 and D-5
  touch its size.

## 4. Phases

Each phase is one commit and passes `npm run -s test:gate`. Timing: 50-run average of
`smllm graph --config …` with the release binary, on `/workspaces` and on local disk (a copy
under `target/`), plus `strace -f -e trace=openat` counting the files opened. Recorded before A1,
after A1 and after A2.

| Phase | Name | Items | Proof |
| ----- | ---- | ----- | ----- |
| A1 | Load mode | P-1, P-2, P-10 | `validate` and `compile` snapshots unchanged; a run-mode test with a missing prompt file still errors; load opens only the config and machine files; per-stage timing; before/after timing |
| A2 | Hooks read less | P-3, P-4, P-5, P-9, D-9 (store helper) | session tests unchanged; strace: user-prompt-submit and an idle stop open no machine file; timing |
| A3 | Engine | P-6, P-11, D-1, D-2, D-3, D-4 | engine and instance tests unchanged except D-4's snapshots; wasm size |
| A4 | Runner and store | P-7, P-8, P-12 | host tests; a test that a quick command's output is complete; the duplicate-ref test still fails a duplicate |
| A5 | Shared helpers | D-5, D-6, D-7, D-8, D-9 (rest) | MCP and wasm smoke tests; harness tests |
| A6 | Test helpers | D-10 | test count unchanged |
| A7 | Docs and outcome | NFR-2_AC-1 figures (both disks), DESIGN-CFG (modes), CFG-7 portable pattern note (D4-3), DESIGN-DEC (runner), ARCHITECTURE change log, this plan's §7 | — |

## 5. Open questions

None; answered in §6 by the usual practice.

## 6. Decisions

- D4-1 (Q1, pattern check): keep validating with `regex::Regex::new`, the standard check, and
  compile each pattern once, reusing it for the enum check (option c). No new dependency, no
  behaviour change; the load keeps the 0.8 ms.
- D4-2 (Q2, instance history): defer the `done/` layout (YAGNI). It costs nothing at today's sizes,
  and changing the store layout needs its own plan.
- D4-3 (Q3, `\d`): no code change. JSON Schema, which `params` follows, says patterns should be
  ECMA-262 and recommends a portable subset for interoperability, without `\d`, `\w` or `\s`.
  CFG-7 documents that the hosts' regex engines differ outside that subset (Rust's `\d` is
  Unicode, JavaScript's is ASCII), so write `[0-9]` for ASCII digits.
- D4-4 (Q4, prompt files in `Run`): fail fast. `Run` checks that each named prompt file exists,
  once per path; a missing file fails the machine at load, as now. Only reads and the
  `enter-<STATE>.md` probes go.

- D4-5 (Q5, raised in A1): keep the release profile at `opt-level = "z"`. Measured per load:
  YAML parsing 2.3 ms at `z`; `opt-level = 3` for `serde-saphyr`, `granit-parser`,
  `regex-automata` and `regex-syntax` would save about 1.4 ms for +330 KB (+9.5%), for the whole
  binary +1.5 MB. Binary size wins.

## 7. Outcome

| Phase | Commit / result |
| ----- | --------------- |
| A1 | `Mode { Run, Check, Inline }`; one compile per pattern; shape keys from one shared schema (first file 0.5 → 0.23 ms). Stage timing (per load, `z`): read 0.7 ms, YAML 2.3, shape 0.35, lower 1.0 (patterns). `graph`: 86 → 17 file accesses; `/workspaces` 11.4 → 5.4 ms; local disk 4.75 → 4.35 ms |
| A2 | `Session::may_stop` (core, used by `Engine::stop` and the hook); hooks read the session once; user-prompt-submit and an allowed stop load no config; `Runtime::bound`, `FsStore::user`; the runtime moves the config into the engine (`sources`, `findings` fields). Idle session, `/workspaces`: user-prompt-submit 11.3 → 0.68 ms, stop 10.9 → 0.69 ms; 0 machine-file accesses |
| A3 | stop reads the held instance once (`view_held`); `Engine::show` behind `view`/`menu`; `Instance::held_by` and `machine::owned` (status, idle list, resume, unmatched); `Turn::run_lists`; `insertion_sort_by` with a comparator; `utils::join`, `Machine::state_names`/`entry_point_names` at 10 sites. D-4's wording merge dropped: the two texts read differently in place ("one of (none: …)" vs "entry points: none"), so output is unchanged. Wasm 256.5 KiB |
| A4 | A passing command returns without waiting for output; a failing one waits on a channel that disconnects when both readers end (no 5 ms polling); `put_instance` scans for a ref clash only when the ref is new; `FsStore::scan` public, `validate` scans each machine once (`unreadable` removed). Tests: a pass with a background process returns in < 150 ms (the old code waited 200 ms); a hand-made ref clash no longer blocks an unchanged ref |
| A5 | Kit `parse_toml` (record, declined ×2, the app's `register`); `sets_ref` in lowering, hoisted out of the per-param loop, the CFG-13 warning reads `set_ref_events`; one `MachineSource`; `output::reply`, `Runtime::all_instances`, `instance show` reads by id first (first match, not last). D-5 dropped: the shared part is a four-line loop over different JSON types, and sharing it needs a new public core function and error text |
| A6 | `tests/common`: `Mcp` (`call`, `send`, `notify`, `next`, `tool`, `close`, `finish`, `at_2026_07_28`) behind `World::mcp`, `World::start`; the three MCP tests and the status line test use them. Temp dirs: app `src/test_support.rs` (4 unit tests), `validate.rs` `temp_dir` (4 tests); the kit already had one of each. 170 tests, as before |
| A7 | DESIGN-CFG (load modes, patterns are host regexes), DESIGN-DEC (runner), DESIGN-NFR (NFR-2 figures), ARCHITECTURE 0.4.0, CHANGELOG; the pattern doc (and `schema/smllm.schema.json`) tells authors to write `[0-9]`; `cargo fmt`; workspace clippy clean for `x86_64-pc-windows-gnu`. `mcp_pipelined_calls_on_one_session` failed 1 run in 3 under the full suite: it pipelined `enter` with the yields, and the server does not order pipelined calls, so yields could reach idle first. It now waits for `enter`, then pipelines 60 yields; 10/10 full-suite runs pass |
