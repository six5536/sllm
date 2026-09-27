# PLAN-003: Fix the full-repo code review

| Meta               | Value                                                                                  |
| ------------------ | -------------------------------------------------------------------------------------- |
| Status             | completed (P0–P12 implemented 2026-09-27, §7)                                          |
| Workflow direction | bottom-up (code findings → specs updated per phase)                                    |
| Traces to          | TURN-4/6/12, INST-3/7/8, IDLE, STO-3, HOST-5/7/8, DEC-6, CFG, CLI-9, NFR-1/4/5/8, TEST |

## 1. Goal

Fix every confirmed finding of the 2026-09-26 `/code-review max` of the whole repo (about 35 bugs
plus the DRY and idiom findings), CI configuration included. Out of scope: proving the fixes on
GitHub. CI changes are checked locally only (the workflows' install step with Node 26, `actionlint` if present),
not by pushing and watching runs.

**Safety rule until F0 lands:** run no `cargo test`/`nextest`/`npm test`/`test:gate`/`coverage`
in this container (F0 kills every process the user owns).

## 2. Findings (requirements)

Ids are this plan's. Severity order from the review; `R-` = review rank.

| ID   | Finding (file) → fix                                                                                    | Traces |
| ---- | ------------------------------------------------------------------------------------------------------- | ------ |
| F0   | R-1 `host.rs:178` `kill -KILL -<pid>` parsed as an option → `kill(-1)` → `kill -s KILL -- -<pid>`, only after checking the group is ours and `kill` exists (D3-1); test the group dies | DEC-6, NFR-4 |
| F1   | R-5 kit `write_if_changed` and `store::write_atomic` replace symlinks, reset mode → write through the link target, keep the mode | STO-3, HOST-5 |
| F2   | R-8 temp name `.{name}.tmp{pid}` shared by concurrent writes → unique per write (pid + counter), clean up on error | STO-3 |
| F3   | R-8 `smllm mcp` fires tool calls on one session concurrently → serialise calls in the server (one at a time) | STO-3, HOST-5 |
| F4   | Two sessions entering the same new ref create duplicate instances → create under the machine lock, re-check the ref | INST-3, STO-3 |
| F5   | One corrupt instance file breaks idle views and status line, `validate` says clean → skip it with a warning; `validate` reports it | IDLE, STL-4 |
| F6   | R-2 `stop` releases on `stop_hook_active` even after an event was fired → session records "blocked since last event"; Runaway only when set | TURN-4_AC-1, TURN-6_AC-1 |
| F7   | R-9 unconfigured held machine rejects every event, `park` included → allow `park`/`yield`/idle `enter` out of it | INST-7, IDLE |
| F8   | R-10 broken project machine or config silently falls back to the user machine of the same id → the id is unconfigured (error finding), no fallback | CFG, INST-7 |
| F9   | R-6 refs and echoed input written raw into `<smllm>` text → one escape for untrusted text (`<`, newlines) at every echo site (`text.rs`, `offer.rs:254`, `idle.rs:192/224/233`) | TURN-12_AC-1 |
| F10  | Set-ref checked before the transition is chosen → check only the chosen branch's `setRef` | INST-3, ACT |
| F11  | R-11 `.smllm/plan.smllm.yaml` DRAFT dead end once the ref is set → setting a ref to the value it already has is a no-op (D3-3); the machine is unchanged | INST-3_AC-1 |
| F12  | Takeover: view says idle but the old session still holds the instance → drop the hold with the message | INST-7 |
| F13  | Rejected `enter` without a key shows a key it never saved → show no key, or save it | IDLE |
| F14  | Exit prompts dropped on `park`/`suspend` → run them as on any exit | IDLE, ENG |
| F15  | Repeated `--param` at `enter` bypasses the ref pattern → reject duplicate params | IDLE, NFR-5 |
| F16  | `resume` while a machine file is briefly invalid orphans the suspended instance → keep it suspended, report the error | IDLE |
| F17  | Self-transition in the fallback state drops `resume` → keep it offered | IDLE |
| F18  | R-7 `harness install --without` writes smllm's config.toml, which then counts as "configured" → configured only with `[machines]`; `init` accepts a harness-only file | HOST-8_AC-1 |
| F19  | Declined parts still parsed (`--without mcp` blocked by a broken `.mcp.json`) → skip declined parts entirely | HOST-5 |
| F20  | R-12 kit owns whole hook groups → own single hook commands; a group with user commands is "current", `--force` touches only ours | HOST-5 |
| F21  | Lone opening marker leaves a region "edited" forever and `--force` deletes user text → treat as absent, insert a fresh region; never delete unmarked text | HOST-5 |
| F22  | R-3 wasm JS imports lack `catch` → `#[wasm_bindgen(catch)]`, JS exceptions become host errors; engine stays usable | NFR-1, NFR-8 |
| F23  | R-14 wasm `stop()` can't tell block from runaway; `null` is `undefined` → return `{decision: "block"\|"runaway"\|"allow", text?}`; docs | TURN-6 |
| F24  | R-13 Windows `cmd /C` via `args` mangles `"` → `CommandExt::raw_arg` | DEC-6 |
| F25  | Output reader threads block while a background child holds the pipes → stop reading at exit/timeout | DEC-6, NFR-4 |
| F26  | Shape check rejects YAML nulls the schema allows; accepts `{type: prompt}` without params (no line) → align with schema, report with line | CFG |
| F27  | Machine ids differing only in case share a state dir on case-insensitive FS → validation error | CFG |
| F28  | MCP tests don't clear `SMLLM_CONFIG`; tests leak into real dirs on Windows (etcetera ignores XDG/HOME) → one test env helper that isolates both | TEST |
| F29  | R-4 `npm ci` rejects the lock (unpublished exact-pinned optional deps) → `npm install --no-audit --no-fund` in CI and release (D3-2) | release |
| F30  | R-4 Windows test job assumes POSIX sh → `cfg(unix)` on sh-only tests plus Windows twins (D3-5) | TEST |

DRY and idiom (D-items, behaviour unchanged):

| ID  | Finding → fix |
| --- | ------------- |
| D1  | `store::write_atomic` copies the kit's `write_if_changed` → use the kit (after F1/F2); one `read_optional` for the 4 "not found = None" reads |
| D2  | cwd error mapping pasted 7× → `config::cwd()`; stdout error mapping 4× → `output::stdout_err`; `From<HostError>` in `error.rs` replaces 5 hand conversions |
| D3  | `mcp.rs:94-112` and `state.rs:34-46` hand-build runtime + `Bind` → `Runtime::fire` |
| D4  | `entry_prompts` re-implements `turn::lists`; `after_final` and yield branch re-render `block`'s head; `block`'s 8 args → drop `ok` (= `error.is_none()`), take `&Instance` |
| D5  | smllm-format `Level`/`Finding::render` duplicate the kit's `Severity`/`to_line` → use the kit; snapshots of the real user format |
| D6  | `load.rs` `idle_prompt`/`fence_warning` duplicate `lower_prompt`/`check_fences` and the fence list → reuse |
| D7  | `shape.rs` 17 hand key lists → derive from the serde types (one source); remove the dead XState hint table |

## 3. Design sketch

| Where | Change |
| ----- | ------ |
| app `host.rs` | `kill_tree` (unix): `child.try_wait()` is `Ok(None)` and pid > 1, then `kill -s KILL -- -<pid>`; `kill` not found or failing → `child.kill()` alone (D3-1); readers joined with a deadline; Windows `raw_arg` |
| kit `harness/write.rs` | `write_if_changed(path, text)`: resolve symlinks (`fs::canonicalize` when the path is a link), copy the old mode to the temp, temp name `.{name}.{pid}.{n}.tmp` from an `AtomicU64`, remove the temp on error. The app store uses it (D1) |
| app `commands/mcp.rs` | a `tokio::sync::Mutex<()>` held for each `tools/call` |
| core `record::Session` | `blocked: bool` (serde default false): set when `stop` blocks, cleared by any fired event and by `prompt_submitted`; Runaway = `stop_hook_active && blocked` |
| core `engine/machine.rs` | `held()` passes built-ins (`park`, `yield`, idle `enter`) through for `Gone::Unconfigured`; set-ref check after branch choice |
| core `render` | `fn untrusted(s: &str) -> Cow<str>` used at every echo of agent or file input |
| format `load.rs` | a failed project override yields an error finding and removes the id, never the user fallback |
| app `paths::lookup` | "configured" = a config file with a `[machines]` table |
| kit `harness/merge.rs`, `region.rs` | per-command ownership; lone marker handling |
| wasm `lib.rs` | `catch` imports; `stop` returns a JS object |

Store format: `Session.blocked` is additive (serde default), no migration (alpha).

## 4. Phases

Each phase is one commit with `npm run -s test:gate` passing (after F0), specs updated in the
same commit (`@zen-impl`/`@zen-test` markers on new code and tests).

| #   | Phase                 | Items                      | Check beyond the gate |
| --- | --------------------- | -------------------------- | --------------------- |
| P0  | Kill safety           | F0                         | strace-free test: timed-out `sh -c 'sleep 30 & wait'` leaves no child; run the gate only after the fix |
| P1  | Atomic writes         | F1, F2, D1                 | symlink + mode tests in kit and store |
| P2  | Concurrency           | F3, F4                     | pipelined MCP stress test (the review's 300-round repro, shortened) |
| P3  | Stop hook             | F6                         | scripted session: block → fire → stop with `stop_hook_active` blocks again |
| P4  | Engine instances      | F7, F10–F17                | one scripted test per item; F11 also on `.smllm/plan.smllm.yaml`: DRAFT → GRILL → revise → written |
| P5  | Untrusted text        | F9                         | forged-fence ref renders as one fence (snapshot) |
| P6  | Config loading        | F5, F8, F26, F27           | findings snapshots |
| P7  | Harness               | F18–F21                    | install/status tests per item |
| P8  | WebAssembly           | F22, F23                   | `npm run build:wasm` (budget), `npm run test:wasm` with a throwing host |
| P9  | Windows + tests       | F24, F28, F30              | `cargo check --target x86_64-pc-windows-gnu` (zigbuild) if it builds here |
| P10 | CI config             | F29                        | the workflows' install step run in a clean checkout with Node 26 |
| P11 | DRY + idiom           | D2–D7                      | behaviour unchanged: snapshots unchanged except D5's |
| P12 | Docs + outcome        | ARCHITECTURE change log, README/docs for F23, this plan's §7 | — |

## 5. Open questions

None (all resolved in §6).

## 6. Decisions

| #    | Decision |
| ---- | -------- |
| D3-1 | F0 keeps the `kill` program (no `libc` dependency, no `unsafe`), fixed to `kill -s KILL -- -<pid>`, and checks before calling it: (1) the group is still ours: the child is not yet reaped (`try_wait` → `Ok(None)`), so its pid, which is its group id (`process_group(0)`), cannot have been reused; (2) pid > 1, so the call can never be `kill -- -1` or `-0`; (3) `kill` exists: a spawn error (not found) falls back to `child.kill()`. Windows keeps `child.kill()` for now (only `cmd` dies); killing the whole tree there needs a Job Object, a later plan |
| D3-2 | F29: CI and release install with `npm install --no-audit --no-fund`, as CONTRIBUTING already says; accepted: CI may resolve newer Node deps than the lock (few Node deps) |
| D3-3 | F11: INST-3 gains a clause: a ref given at `enter` or by `setRef` that equals the instance's own ref is a no-op, not a rejection; a different value or a ref taken by another instance is still rejected. Fixes any machine that loops back through a ref-setting transition, not just the plan machine |
| D3-4 | F23: wasm `stop()` returns `{decision: "block" \| "runaway" \| "allow", text?}` (`text` present for block and runaway). Breaking for JS hosts; allowed pre-1.0 (alpha), noted in the changelog and the npm README |
| D3-5 | F30: sh-dependent tests become `#[cfg(unix)]`; `cfg(windows)` twins cover what differs there: a `command` guard passing and failing, `SMLLM_*` env, quoting (F24), timeout. F28's test env helper also sets `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`. Full Windows fixtures wait for the Windows plan |
| D3-6 | F13: a rejected key-less `enter` shows no session key, keeping TURN-3's "a rejected keyless call leaves nothing behind" (`engine/api.rs` `fire`) |
| D3-7 | D5: smllm-format depends on agent-harness-kit (user approved; internal crate, nothing new in the build): findings use the kit's `Severity` and `Report`, and the format tests snapshot the lines `validate` prints |

## 7. Outcome

| Phase | Commit / result |
| ----- | --------------- |
| P0 | 1b493b6: `kill -s KILL -- -<pgid>` after `try_wait` and pid > 1 checks; test kills a grandchild and spares a bystander, fails without the group kill |
| P1 | 2b91c7e: kit `fs::{read_text, write_atomic}` (symlinks followed, mode kept, unique temp, temp removed on error); the app store uses it |
| P2 | 98f249b: MCP calls serialised; ref uniqueness checked under the machine lock; 60-call pipelined test fails without the lock |
| P3 | 0d1fa5a: `Session.blocked`; runaway only when blocked and no event since |
| P4 | 1d301c5: F7, F10–F17; nine tests in `smllm-core/tests/instances.rs`, all failing on the old engine |
| P5 | 512a3ed: `Block::line` escapes line breaks and smllm's tags; the unknown-session error too |
| P6 | 943e1ee: F5, F8, F26, F27 |
| P7 | b3c531a: F18–F21 |
| P8 | 1e8f735: `catch` imports, `stop()` → `{decision, text?}` JSON; wasm 261 KiB; smoke test with a throwing host (600 calls) fails on the old wrapper |
| P9 | b92838f: `raw_arg`; `World::user_dirs` / `World::mcp`; sh-only tests `cfg(unix)`, a Windows twin; workspace clippy clean for `x86_64-pc-windows-gnu` |
| P10 | e48a20c: `npm install --no-audit --no-fund` in CI and release; clean clone: `npm ci` exits 1, the new step 0 |
| P11 | 4972627 (D2–D4, D6, D7), 96becd8 (D5) |
| P12 | this commit: CHANGELOG, ARCHITECTURE change log, this section |

Changes from §2–§5 while implementing:

- F9 escapes in one place, `Block::line` (every engine line), rather than at each echo site.
- F23 returns a JSON string like every other wasm method, not a JS object.
- F13 renders `no session · idle` and a call line without a session.
- F8 finds a broken file's id from its top-level `id:` line; a later config that does not parse withdraws every earlier machine.
- The P6 `refusal` property test now expects a declined part's broken file not to block (F19).
- Not verified: CI on GitHub (out of scope, §1); Windows tests only compile-checked here.
- Commits from P8 on are unsigned: the signing agent refused (the plan machine's fallback).
