# PLAN-001: smllm foundation

| Meta               | Value                                                                                  |
| ------------------ | -------------------------------------------------------------------------------------- |
| Status             | completed (P0–P7 implemented and verified 2026-09-25, §15) |
| Workflow direction | top-down (no ARCHITECTURE.md yet — this plan seeds it)                                 |
| Traces to          | `.zen/specs/ARCHITECTURE.md`, `REQ-*`/`DESIGN-*` for CFG, INST, IDLE, ENG, DEC, ACT, TURN, STO, HOST, CLI, NFR, TEST |

## 1. What smllm is

**smllm = state machines for LLM agents.** A Rust CLI + library that puts declarative state
machines (YAML) in charge of an LLM coding agent's turn loop. Harness-agnostic; Claude Code first.

Terms: **state machine**, **state**, **event**, **param** (a value an event carries),
**instance** (one piece of work moving through a state machine), **ref** (an instance's external
id, e.g. an issue id), **session** (one harness conversation, identified by an smllm session key).

- **Entering a state** → the agent gets a header (session, state machine, state, visit, instance,
  arriving event + params) and the state's instructions.
- **Trying to stop** → the stop hook shows the offered events with guidance and params, and asks
  the agent to call the **one tool**: `smllm({ session, event?, params? })`.
- **The tool call** validates, transitions (possibly through guarded transitions and eventless `always` states), saves, and returns
  the next state's entry block. With no `event` it is a read-only "where am I" query.
- A session starts in smllm's **idle** state. From idle the agent `enter`s an instance of a state
  machine; it leaves with `park`, `unmatched` (detour) or by reaching a final state.

## 2. Rename sllm → smllm

Names checked free (2026-09-24): crates.io `smllm`, `smllm-core`; npm `smllm`; GitHub `six5536/smllm`.

| Step | Scope                                                                                         |
| ---- | --------------------------------------------------------------------------------------------- |
| R1   | `crates/app/sllm` → `crates/app/smllm`, `crates/lib/sllm-core` → `crates/lib/smllm-core`     |
| R2   | Package/bin names, `default-members`, repository/homepage URLs in `Cargo.toml`s               |
| R3   | `packages/sllm*` → `packages/smllm*` (launcher + 5 platform packages), `bin/smllm.js`         |
| R4   | `scripts/*.mjs`, `.github/workflows/*`, README, CHANGELOG, insta snapshot names               |
| R5   | Rename GitHub repo + local dir (user action); grep for leftover `sllm` to confirm none remain |
| R6   | Add `crates/lib/smllm-format` (std) beside `smllm-core` (no_std) — §11                        |
| R7   | Add `crates/lib/smllm-wasm` (`cdylib`, `wasm-bindgen`, `publish = false`) + `packages/smllm-wasm` (npm); one exported fn + Node smoke test; in the release pipeline |
| R8   | CI: `cargo build -p smllm-core --no-default-features --target wasm32-unknown-unknown`; build `smllm-wasm`; JS smoke test; report `.wasm` size per PR |
| R9   | `.zen/rules/rust-rules.md`: rewrite the WASM section for `smllm-core` (drop bitmark-parser/PLAN-182; new build command; helpers created in `smllm-core` when first needed) |
| R10  | `submodules/sokf` git submodule (done, `9c93f37`) — source for `agent-harness-kit` (§11)       |

Replace the `hello` placeholder command once the real CLI lands.

## 3. State machine format (CFG) — a strict subset of XState v5

A machine file is an **XState v5 machine config written in YAML**; smllm-only data lives under
XState's `meta`. Stately's editor/visualiser can open it; a JS host could run it with XState, given implementations of the guard and action types.

```yaml
id: dev                                    # unique within the config
description: Fix an issue end to end.      # shown in the idle list
initial: TRIAGE                            # where a new instance starts
meta:
  smllm: 1                                 # smllm format version
  instance:
    noun: issue                            # used in all smllm-generated text
    ref:
      param: issueId                       # the ref's param name (enter + setRef)
      description: "The GitHub issue id, e.g. GH-123."
      pattern: "^GH-\\d+$"
  events:                                  # event types, declared once
    reject:
      description: "Select when the work needs changes."    # default guidance
      params:                              # JSON Schema object subset
        type: object
        properties:
          reason: { type: string, description: "What is missing." }
          severity: { type: string, enum: [minor, major], description: "How much rework." }
        required: [reason]
    submit:
      params:
        type: object
        properties:
          summary: { type: string, description: "One-line commit message for the change." }
        required: [summary]
  sharedActions:                           # was promptInjections; any action kind
    - states: [WORK, REVIEW]
      position: before
      entry: { type: prompt, params: { text: "Work on one issue at a time." } }

states:
  TRIAGE:
    description: Decide whether the issue is real.
    meta: { entryPoint: true }             # may be entered directly from idle
    entry: { type: prompt, params: { file: triage.md } }   # default: enter-<STATE>.md
    on:
      issueCreated: { target: WORK, actions: setRef }      # its ref param is implied
      accept: CHECK
  CHECK:                                   # eventless: no agent turn
    always:
      - guard: { type: command, params: { run: 'gh issue view "$SMLLM_REF"' } }
        target: WORK
      - target: TRIAGE
  WORK:
    entry:
      - { type: command, params: { run: 'git switch "issue/$SMLLM_REF" 2>/dev/null || git switch -c "issue/$SMLLM_REF"' } }
      - { type: prompt, params: { file: work.md } }
    on:
      submit:                              # guarded transitions; last has no guard
        - guard: { type: command, params: { run: "cargo test --quiet", timeoutSecs: 300 } }
          target: REVIEW
          actions: { type: command, params: { run: 'git commit -am "$SMLLM_PARAM_SUMMARY"' } }
        - guard: { type: visits, params: { state: WORK, atLeast: 3 } }
          target: ESCALATE
        - target: WORK
          reenter: true                    # re-run entry, count a visit
  REVIEW:
    on:
      approve:
        target: DONE
        actions:
          - { type: command, params: { run: "gh pr merge --auto" } }
          - { type: prompt, params: { text: "Confirm the PR merged." } }
      reject:
        target: WORK
        description: "Select when a review checklist item fails."   # per-state guidance
    meta:
      paramDescriptions: { reject: { reason: "Which checklist item failed." } }
  ESCALATE:
    on: { resolved: WORK }
  DONE:
    type: final
```

| ID     | Requirement                                                                                                  |
| ------ | ------------------------------------------------------------------------------------------------------------ |
| CFG-1  | Valid XState v5 config (YAML); `meta.smllm: 1`; unknown keys rejected; camelCase keys                         |
| CFG-2  | Supported XState: `id`, `initial`, `description`, `meta`, `states`, `on`, `always`, `entry`, `exit`, `actions`, `guard`, `target`, `reenter`, `type: final`. Rejected (v1): nested `states`, `type: parallel`/`history`, `after`, `invoke`, `context`/`assign`, `output`, `tags` |
| CFG-3  | XState semantics: guarded transition arrays (first match), `always` (eventless), targetless = internal, self-target re-enters only with `reenter: true` |
| CFG-4  | Actions `{type, params}`: `prompt` (`text` \| `file`), `command` (`run`, `timeoutSecs`, `cwd`), `setRef`. A string is shorthand for a param-less action (`setRef`) |
| CFG-5  | Guards `{type, params}`: `command` (`run`, `timeoutSecs`, `cwd`), `visits` (`state`, `atLeast`; counts entries so far, including the current one) |
| CFG-6  | `meta.instance`: `noun`, `ref` (`param`, `description`, `pattern`); defaults noun `instance`, param `ref`    |
| CFG-7  | `meta.events.<name>`: `description` (default guidance), `params` = JSON Schema subset (`type: object`, `properties` of `type: string` with `description`, `enum`, `pattern`; `required`) |
| CFG-8  | Per-state overrides: transition `description` (guidance); state `meta.paramDescriptions` (param prompts only, never definitions) |
| CFG-9  | Undeclared events are allowed (no params); declared-but-unused → warning; any transition with `setRef` makes the ref param required on that event type |
| CFG-10 | State `meta`: `entryPoint`, `paramDescriptions`, `fallback` (see IDLE-2); machine `meta`: `smllm`, `instance`, `events`, `sharedActions` |
| CFG-11 | `meta.sharedActions`: ordered, `before`/`after` a state's own `entry`/`exit`, every occurrence kept           |
| CFG-12 | `type: final` states: optional, no `on`/`always`; each reachable; none at all → info finding                  |
| CFG-13 | Checks: last transition of each guarded array / `always` unguarded; no `always` loops; name clashes (ref param vs `stateMachine`/`state`/event params); built-in event names reserved; at most one `meta.fallback` state, which must not be `type: final` |
| CFG-14 | Findings: file, YAML path, fix hint; levels error / warning / info; all collected                             |
| CFG-15 | JSON Schema of the format generated from the Rust types (`smllm info schema`); template `<id>.smllm.yaml`    |
| CFG-16 | Tool schema: `params` is `{[name]: string}`; non-strings rejected naming the param; other JSON Schema types may come later |
| CFG-17 | Braces are literal: nothing in user text is a template                                                        |

Superdev v2 → smllm: `schemaVersion` → `meta.smllm`; `initialState` → `initial`; `events` → `on`;
`actions.onEnter` → `entry` (prompt action); `event_instructions` → `meta.events.*.description` /
transition `description`; `prompt_injections` → `meta.sharedActions`; `carries`/`effect` → JSON Schema
`params` / `setRef` action; `vocabulary` → `meta.instance`. Removed: `fallbackState`/`fallbackEvent`,
`targetResolutionState`, `yieldEvent`, `$stored`, `$named`, `history`.

## 4. Instances (INST)

| ID     | Requirement                                                                                                 |
| ------ | ----------------------------------------------------------------------------------------------------------- |
| INST-1 | Every state machine session works on an instance; there is no "inside a machine with no instance" state     |
| INST-2 | Instance id: generated (`i-xxxx`), permanent internal key for state and history                             |
| INST-3 | Optional ref, unique per state machine, set once: at `enter`, or by a `setRef` action; setting again = error |
| INST-4 | The agent sees one id param (the ref param name): the generated id until a ref is set, then the ref; both resolve |
| INST-5 | Status: active (held by a session) · suspended (detour) · parked · completed (final state reached, kept)    |
| INST-6 | One session holds an instance at a time; `enter` takes over, reporting the previous holder and its last activity |
| INST-7 | A superseded session's next call gets "moved to session …" and drops to idle                                |
| INST-8 | Writes are versioned under a file lock; a concurrent loser gets the "moved" error                          |
| INST-9 | Completed instances are never deleted by the agent                                                         |
| INST-10 | Reopen: `enter` on a completed instance without `state` → no move, result lists entry points; with a `meta.entryPoint` `state` → active again, header "reopened" |
| INST-11 | Visit counts and history carry on across reopen                                                            |

## 5. Idle + built-in events (IDLE)

Idle is smllm's own state, outside every state machine. Its entry block is the **idle list**:
each state machine (id, description, entry points, ref param + prompt), the suspended instance,
and parked instances (ref or id, saved state).

| Built-in    | Offered in       | Does                                                                                   |
| ----------- | ---------------- | -------------------------------------------------------------------------------------- |
| `enter`     | idle             | params `stateMachine` (req), `<ref param>`, `state`. New → `state` (must be `meta.entryPoint`) or `initial`; existing → saved state, or `state` (`meta.entryPoint`) as a recorded jump |
| `resume`    | idle if suspended; a `meta.fallback` state | Re-enter the interrupted state (counts a visit)             |
| `park`      | every state      | Instance → parked; session → idle                                                      |
| `unmatched` | every state      | Instance → suspended; session → idle (detour); `resume` returns                        |
| `yield`     | every state      | Stay, no re-entry, no visit; the turn may end. `note` param (optional)                 |

| ID     | Requirement                                                                                                  |
| ------ | ------------------------------------------------------------------------------------------------------------ |
| IDLE-1 | Built-in names are fixed; a state machine may override their guidance (`meta.events.<name>.description`), not their params |
| IDLE-2 | A state machine may mark one state `meta.fallback: true`: `unmatched` then goes there (instance stays active, interrupted state remembered) instead of idle, and that state is offered the built-in `resume` to return |
| IDLE-3 | Saved state missing (config changed) on `enter`/`resume` → no move; result lists all states; `enter` with any `state` repairs |
| IDLE-4 | `validate` warns about saved instances whose state no longer exists                                          |
| IDLE-5 | Entering a `type: final` state: run its entry actions (its prompt is shown), mark completed, session → idle   |
| IDLE-6 | A jump of an existing instance shows `Arrived by: enter (jump from <STATE>)` and is recorded in history       |

## 6. Engine, branching + actions (ENG, DEC, ACT)

| ID    | Requirement                                                                                                      |
| ----- | ---------------------------------------------------------------------------------------------------------------- |
| ENG-1 | Offered events = the state's `on` events ∪ built-ins (`park`, `unmatched`, `yield`); none in final states       |
| ENG-2 | Fire: check offered + params → pick transition (guards) → exit/actions/entry (ACT-1) → `always` states → save  |
| ENG-3 | Every state *entry* counts a visit per instance + state (incl. `reenter: true`, `resume`; not targetless or non-reentering self transitions) |
| ENG-4 | Instruction files are read at request time; YAML edits need a reload                                            |
| ENG-5 | No `event` → read-only: entry block + events list; no transition, no visit, no history, doesn't count as a yield |
| DEC-1 | Guarded transition arrays: ordered, first match wins (XState)                                                    |
| DEC-2 | `always` states are left immediately and never rested in; chains length-capped; `always`-only loops rejected   |
| DEC-3 | Guard types v1: `command`, `visits` (CFG-5)                                                                      |
| DEC-4 | `command` `run`: string → system shell (`sh -c` / `cmd /C`); array → exec, no shell                             |
| DEC-5 | Exit 0 = true; timeout / spawn failure = false with reason; `timeoutSecs` default 60; output tail captured       |
| DEC-6 | Env only, never interpolation: `SMLLM_SESSION`, `SMLLM_MACHINE`, `SMLLM_STATE`, `SMLLM_EVENT`, `SMLLM_INSTANCE`, `SMLLM_REF`, `SMLLM_PARAM_<NAME>` |
| DEC-7 | cwd = the session's working dir (recorded at bind); `params.cwd` overrides, relative to the YAML              |
| DEC-8 | Trace (event, transition taken, guard results, output tail) → history and the next header                       |
| DEC-9 | Array-form commands get no shell: `$SMLLM_*` is not expanded there (use the string form or read env in a script) |
| ACT-1 | Order (XState/SCXML): guards pick the transition → source `exit` → transition `actions` → target `entry` → (`always` states, repeat); targetless or non-`reenter` self transitions run only their `actions` |
| ACT-2 | Command actions run first, in order; prompt actions are concatenated in the same order into one `<instructions>` block |
| ACT-3 | Actions never block a transition: a failed/timed-out command skips the rest of its list and is reported in the next header |
| ACT-4 | Commands share the guard runner, env (DEC-6, plus `SMLLM_FROM`, `SMLLM_TO`), forms, timeouts and cwd           |
| ACT-5 | Built-ins: `park`/`unmatched` run the current state's `exit`; `enter`/`resume` run the target's `entry`; `yield` is targetless and runs none |
| ACT-6 | Hosts supply action kinds via an `Action` trait in the core; unsupported kinds → finding at load              |

## 7. The agent interaction loop (TURN)

```
         ┌────────── tool result: entry block (header + instructions) ◀─────────┐
         ▼                                                                       │
  agent works ──▶ tries to stop ──▶ stop hook: events list ──▶ smllm({session, event, params})
         ▲               │                                                       │ invalid → error
         │               └── allowed: yielded, or in idle                        │ + events list
         └── session start / resume / compact: entry block of the current state (or idle list)
```

Everything smllm emits is wrapped in one `<smllm>` fence; author text sits in `<instructions>`,
smllm's menu in `<events>`, so the model can tell them apart.

Entry block:

```
<smllm>
session sm-k7f3q2 · dev › WORK (visit 3) · issue GH-123
Arrived by: reject from REVIEW
Params: reason = "no tests for the error path"
Action failed: entry[0] exited 128: fatal: a branch named 'issue/GH-123' already exists
<instructions>
<prompt actions: exit → transition actions → sharedActions before → entry → sharedActions after>
</instructions>
</smllm>
```

Events list:

```
<smllm>
session sm-k7f3q2 · dev › REVIEW · issue GH-123
Fire one event: smllm({ session: "sm-k7f3q2", event, params })
<events>
- approve — Select when the work is complete and correct.
- reject — Select when a review checklist item fails.
    reason (required): Which checklist item failed.
    severity (optional, one of: minor, major): How much rework.
- yield — Stop for now and stay in REVIEW. note (optional): what you're waiting for.
- park — Put GH-123 aside and return to idle.
- unmatched — The request fits none of these; handle it from idle, then resume.
</events>
</smllm>
```

Errors: `<smllm>` + header + `error: <what was wrong>` + `<events>…</events>`.

| ID      | Requirement                                                                                                  |
| ------- | ------------------------------------------------------------------------------------------------------------ |
| TURN-1  | Header: session key, machine › state, `(visit n)` from the 2nd visit, noun + id/ref; then arriving event, params, transition trace, failed actions |
| TURN-2  | Events list: machine events first, then built-ins; params with required/optional, enum values or pattern, prompt |
| TURN-3  | Valid call → next entry block; invalid → error block + events list, no transition                            |
| TURN-4  | Stop hook: in a machine state, block with the events list unless the agent yielded this turn (every other resting place is idle) |
| TURN-5  | In idle → allow stop                                                                                         |
| TURN-6  | Runaway guard: Claude Code's documented `stop_hook_active` input is the primary signal (already continuing because of a stop hook and still no event → allow stop, list to stderr); sokf's same-report hash is the fallback for harnesses without it |
| TURN-7  | Idle list is delivered on entering idle only (session start/resume/compact, `park`, `unmatched`, final), never per prompt |
| TURN-8  | User prompt clears the yielded flag; injects nothing                                                         |
| TURN-9  | Instructions part tells the agent: fire `yield` before stopping to ask; call with no event to re-orient; only the main agent calls `smllm`, never pass the key to subagents |
| TURN-10 | Params are not stored on the instance; only the next header and history carry them                          |
| TURN-11 | The text format is a stable contract, covered by snapshot tests                                              |
| TURN-12 | All smllm output is fenced in `<smllm>…</smllm>`; author text in `<instructions>`, the menu in `<events>`; author text containing `</smllm>`, `</instructions>` or `</events>` → validation warning |

## 8. Sessions, storage + harnesses (STO, HOST)

| Where                                         | What                                                                  |
| --------------------------------------------- | --------------------------------------------------------------------- |
| `<user state dir>/smllm/sessions/<key>.json`  | Session: harness binding, bound config path, working dir, held instance, flags |
| `<user state dir>/smllm/bindings/`            | Harness session id → key (e.g. Claude Code `session_id`)              |
| `<config dir>/state/` (+ `.gitignore`)        | Instances: state, visits, status, holder, version; history JSONL      |

| ID     | Requirement                                                                                                   |
| ------ | ------------------------------------------------------------------------------------------------------------- |
| STO-1  | Sessions are user-level; instances live beside their config                                                   |
| STO-2  | A session is bound to the config found (from the harness cwd) when it is created; all later calls use it      |
| STO-3  | Atomic writes + file lock + instance version (INST-8)                                                         |
| HOST-1 | Core protocol: `bind(harness, hostSessionId?)`, `view(key)`, `menu(key)`, `fire(key, event, params)`          |
| HOST-2 | Session key: short, random; shown in every header                                                             |
| HOST-3 | Unknown/missing key → error with hint; no-hook harnesses may call with no key + `enter` to bind a new session |
| HOST-4 | `/clear` or a fork → new harness session id → new key in idle; held instances are taken over via `enter`       |
| HOST-5 | Surfaces: MCP tool (`smllm mcp`) and CLI (`smllm fire`), same core                                            |
| HOST-6 | Claude Code: `harness install claude [--scope project\|user]`, parts as sokf (table below); **also** a Claude Code plugin (hooks + MCP server) as the one-command install |
| HOST-7 | Stop answer as sokf: `{}` or `{"decision":"block","reason":"<smllm>…</smllm>"}`; failure → exit 1, stderr only |
| HOST-8 | No config found → every hook answers `{}`                                                                     |
| HOST-9 | Spike (P1): confirm SessionStart/UserPromptSubmit injection and whether `--resume` keeps `session_id`        |
| HOST-10 | Target file, as sokf: no `AGENTS.md` → `CLAUDE.md` (created if absent); `AGENTS.md` only → it; both and `CLAUDE.md` has an `@AGENTS.md` line → `AGENTS.md`; both, no import → `CLAUDE.md` |
| HOST-11 | Block between `<!-- smllm:harness -->` / `<!-- /smllm:harness -->`: replace between first markers; else append after one blank line; empty/absent file = the block; keep line endings; write only on change; blank line after the opening marker; compared by words (whitespace-insensitive); edited → only with `--force` |
| HOST-12 | MCP tool description carries the full rules (TURN-9) for every MCP harness; stable contract, snapshot-tested |

| Part           | project scope                               | user scope                   | Content                                   |
| -------------- | ------------------------------------------- | ---------------------------- | ----------------------------------------- |
| `instructions` | `AGENTS.md` or `CLAUDE.md` in `.smllm/`'s parent (HOST-10) | same rule in `~/.claude/` | Tiny orientation block (HOST-11)  |
| `mcp`          | `.mcp.json` (merge; the documented project file — Claude Code asks the user to approve it) | via `claude mcp add-json --scope user` / `claude mcp remove`, never by editing `~/.claude.json` (Claude Code's internal state file) | `smllm` server → `smllm mcp` |
| `hooks`        | `.claude/settings.json` (merge)             | `~/.claude/settings.json`    | `session-start`, `user-prompt-submit`, `stop` → `smllm harness hook claude <HOOK>` |
| `permissions`  | `.claude/settings.json` (merge)             | `~/.claude/settings.json`    | Allow the `smllm` MCP tool                |

## 9. CLI (consistent with `sokf`)

sokf's output conventions, not its repository ones (smllm is not tied to a repo).

| Convention     | smllm                                                                                                   |
| -------------- | ------------------------------------------------------------------------------------------------------- |
| Global options | `--config <FILE>`, `-V/--version`; before or after the command                                          |
| Config lookup  | `--config` / `SMLLM_CONFIG` → only that file. Else **combined**: `~/.config/smllm/config.toml` + nearest `.smllm/config.toml` upward; same `id` → project wins (info finding); project `[idle]` replaces user's; each machine's instances beside its own config; the session records both paths |
| Exit codes     | 0 no error · 1 errors found (config errors, rejected event, failed hook) · 2 usage/internal error       |
| Output         | `error: <message>` on stderr; `--json` = one object, nothing else, with a published schema              |
| Findings       | `<path>:<line>: <level>: <message> (<authority>)`; counts at the end; `--warnings`, `--info` list them  |
| Tool config    | `config.toml` (kebab-case keys), beside it `harness.toml`                                               |

```toml
# .smllm/config.toml — paths relative to this file
[machines]
files = ["dev.smllm.yaml", "plan.smllm.yaml"]

[idle]
on-enter = { file = "idle.md" }   # optional extra text for the idle list
```

| ID     | Command                                                                     | Purpose                                        |
| ------ | --------------------------------------------------------------------------- | ---------------------------------------------- |
| CLI-1  | `smllm init [--user] [--json]`                                              | Create `.smllm/config.toml` or the user config |
| CLI-2  | `smllm new <ID> [--dir DIR] [--write] [--json]`                             | Scaffold `<id>.smllm.yaml`; `--write` writes + registers |
| CLI-3  | `smllm validate [PATHS] [--json] [--warnings] [--info]`                     | Check config + state machines + saved instances |
| CLI-4  | `smllm fire --session KEY <EVENT> [--param KEY=VALUE]… [--json]`            | The tool, for shell-only harnesses             |
| CLI-5  | `smllm session list\|show <KEY> [--json]`                                   | `show` = the tool's no-event view              |
| CLI-6  | `smllm instance list\|show <ID> [--json]`                                   | Instances + history                            |
| CLI-7  | `smllm harness install\|status <NAME> [--scope S] [--without PART]… [--force] [--json]` | Harness integration          |
| CLI-8  | `smllm harness hook <NAME> <HOOK>` (hidden)                                 | Hook entry                                     |
| CLI-9  | `smllm mcp` (hidden)                                                        | Stdio MCP server, one tool                     |
| CLI-10 | `smllm graph [ID] [--json\|--mermaid]`                                      | States + transitions (guards shown); Stately can also open the file |
| CLI-11 | `smllm info schema`                                                         | JSON Schema of the state machine format        |
| CLI-12 | `completions <SHELL>`, hidden `man`                                         | Stock                                          |
| CLI-13 | `smllm compile <FILE> [-o OUT]`                                             | Validated model as compact JSON, for `smllm-wasm` hosts |

## 10. Non-functional requirements

| ID    | Requirement                                                                                          |
| ----- | ---------------------------------------------------------------------------------------------------- |
| NFR-1 | `smllm-core` is `no_std` + `alloc`, follows the WASM rules, and has no IO/time/randomness: hosts supply `Store`, `Guard`, `Action`, `InstructionSource`, `Clock`, `Ids` |
| NFR-2 | Hook latency < 50 ms excluding guard and action commands                                            |
| NFR-3 | Deterministic: same state + input + guard results → same output (property-tested)                    |
| NFR-4 | Hook failures never wedge the agent (HOST-7)                                                         |
| NFR-5 | State machine files are trusted like a Makefile (guards run commands); LLM input reaches them only as env |
| NFR-6 | Exit 2 leaves every file as found; JSON forms/options/exit codes change only in a minor release before 1.0 |
| NFR-7 | Files ≤ 800 lines; `.zen/rules/rust-rules.md`; ships via the existing release pipeline               |
| NFR-8 | `smllm-core` builds for `wasm32-unknown-unknown` in CI; `.wasm` size reported per PR; budget set after P3 |
| NFR-9 | Unsupported guard or action types for a host (e.g. `command` in a browser) → finding when the machine is loaded      |

### Testing (TEST)

| ID     | Layer                | What                                                                                          |
| ------ | -------------------- | --------------------------------------------------------------------------------------------- |
| TEST-1 | Core                 | Unit; snapshots of all agent text (entry, events, errors, tool description, instructions block); properties: invalid call never changes state, determinism, visits monotonic, never rest in an `always` state |
| TEST-2 | Scripted sessions    | Fake agent drives `harness hook claude …` JSON + `fire` / MCP over stdio through the showcase and a `dev` example: enter, setRef, guarded transitions (`true`/`false`), park, detour/resume, takeover, final, reopen |
| TEST-3 | wasm                 | Node smoke (P0); from P3 one scripted session through the JS API                              |
| TEST-4 | Live model           | `scripts/live-e2e.mjs` with `claude -p`; checks the history path. **Only on explicit human request** — never in CI or hooks |

## 11. Architecture sketch + phases

```
crates/lib/smllm-core    no_std+alloc, WASM rules. model types · engine (fire, view, menu, text render)
                         transitions (guards, actions, `always`) · instances · host traits (Store, Guard, Action, InstructionSource, Clock, Ids)
                         optional `serde` feature on model types
crates/lib/smllm-format  std. YAML/TOML load · validate + findings · JSON Schema · compile → JSON
crates/lib/smllm-wasm    wasm-bindgen over smllm-core only; JS supplies host traits via callbacks
crates/lib/agent-harness-kit  std, no smllm deps, published; factored from sokf for both tools (below)
crates/app/smllm         cli · config lookup · store (user sessions, project instances, locking)
                         guards + actions (command runner, env) · harness/claude · mcp
packages/smllm-wasm      npm package of smllm-wasm (web + bundler targets, wasm-opt)
```

**`agent-harness-kit`** — copied from `submodules/sokf`, generalised; each file headed
`// Derived from sokf <commit> <path>`. sokf adopts it later (tracked in sokf's repo).

| Module             | From sokf                                   | Generalised by                                                       |
| ------------------ | ------------------------------------------- | -------------------------------------------------------------------- |
| `harness::part`    | `harness/profile.rs` types                  | Profiles supplied by the tool; no embedded content                   |
| `harness::state`   | `harness/state.rs`                          | Region compare **by words** (sokf's code only normalises CRLF; its spec CLI-3_AC-16 says words) |
| `harness::region`  | `harness/region.rs`, `target.rs`            | Marker prefix `<tool>:harness`; root = plain dir + relative paths    |
| `harness::merge`   | `harness/merge.rs`                          | Data-driven ops: add-to-array at path; replace-or-append own group under `hooks.<Event>` by command prefix, any events |
| `harness::record`, `write` | `record.rs`, `write.rs`             | Record path/header as parameters; declined-parts store via a trait   |
| `harness::{install,status}` | `api/harness.rs`                   | Generic over a `Tool` trait (name, profiles, paths, declined store)  |
| `hook`             | `hook/claude.rs` `Answer`, `guard.rs`       | Loop guard keyed (per repo / per session) with a store location      |
| `report`           | `report/{finding,collect,text}.rs`          | `Info` severity; authority as string; tool-supplied path type        |
| `cli`              | `app/output.rs`, `main.rs` pieces           | Exit codes, stdout, broken pipe, `error:` runner                     |
| `query` (feature)  | `query/`                                    | Unchanged; unused by smllm for now                                   |

Excluded: `RepoPath`/`RepoRoot` (sokf converts at its boundary).

| #  | Phase                     | Output                                                                  | Traces                     |
| -- | ------------------------- | ----------------------------------------------------------------------- | -------------------------- |
| P0 | Rename + skeleton         | §2 (R1–R10), CI green incl. wasm build + JS smoke test                   | NFR-8                      |
| P0.5 | `agent-harness-kit`     | Crate per the table above, with its own tests (ported from sokf's)       | HOST-6/10/11, CLI conventions |
| P1 | Spike + specs             | HOST-9 findings; `ARCHITECTURE.md`; `REQ-*`/`DESIGN-*` per code         | all                        |
| P2 | Format + validator        | `smllm-format`; `init`, lookup, `validate`, `info schema`, `compile`; converted showcase + bad fixtures | CFG, CLI-1/3/11/13 |
| P3 | Engine                    | Instances, idle, built-ins, text render, snapshot + property tests; `smllm-wasm` exposes `Engine.load/view/events/fire`; size budget set | INST, IDLE, ENG, TURN-1–3, NFR-8 |
| P4 | Store                     | User sessions, project instances, locking/versions, history             | STO                        |
| P5 | Harness + Claude Code     | `mcp`, `fire`, `session`, `harness install\|status\|hook`, Claude Code plugin; end-to-end | HOST, TURN-4–9, CLI-4/5/7–9 |
| P6 | Guards + actions          | Guarded transitions, `always`, `command`/`visits` guards, `command` actions in `entry`/`exit`/`actions`, `sharedActions` | DEC, ACT |
| P7 | Polish                    | `new`, `instance`, `graph`, docs, examples                              | CLI-2/6/10                |

## 12. Dependencies to confirm (rust-rules: none without approval)

| Need          | Candidate                                   | Note                                                     |
| ------------- | ------------------------------------------- | -------------------------------------------------------- |
| YAML          | `serde-saphyr` or `serde_norway`            | `serde_yaml` is unmaintained; `smllm-format` only        |
| wasm bindings | `wasm-bindgen` (+ `js-sys` if needed)       | `smllm-wasm` only                                        |
| wasm tooling  | `wasm-bindgen-cli`, `binaryen` (`wasm-opt`) | devcontainer + CI                                        |
| Small JSON    | `serde_json` vs smaller, measured in P3     | `smllm-wasm` loading compiled machines                   |
| TOML          | as sokf (`toml`; `toml_edit` for in-place)  |                                                          |
| JSON Schema   | `schemars`                                  | CFG-14                                                   |
| JSON merge    | as sokf's `merge` parts                     | keep key order/indent                                    |
| File lock     | std `File::lock` (fallback `fs4`)           |                                                          |
| Cmd timeout   | `wait-timeout` or hand-rolled               |                                                          |
| User dirs     | `etcetera` (XDG on Linux + macOS, Known Folders on Windows) | `$XDG_STATE_HOME` (`~/.local/state`), `$XDG_CONFIG_HOME` (`~/.config`) — honour the env vars |
| Random key    | `getrandom` or hand-rolled from time + pid  |                                                          |
| MCP server    | official Rust SDK `rmcp` vs hand-rolled     | Decide in P1: `rmcp` tracks protocol versions/capabilities (standard); hand-rolled avoids an async runtime in a size-optimised binary |
| Property test | `proptest` (dev)                            |                                                          |

## 13. Decisions

| #   | Decision                                                                                                   |
| --- | ---------------------------------------------------------------------------------------------------------- |
| D1  | Many state machines per config; a session is in at most one; nested/parallel later                         |
| D2  | Instructions arrive on entering a state; the events list at stop time                                      |
| D3  | One tool `smllm({session, event?, params?})`; no event = read-only view                                    |
| D4  | "params", not "variables" (kept free for a future XState-style `context`)                                   |
| D5  | Harness-agnostic; explicit smllm session key on every call                                                 |
| D6  | CLI follows sokf's output conventions, not its repo conventions                                            |
| D7  | Always an instance (INST-1); generated id + set-once ref, one id param named per machine (INST-2–4)         |
| D8  | Built-in detour (`unmatched` → idle, suspended; `resume`) replaces required fallback roles                 |
| D9  | No `$stored`/`$named`/target resolution state; smllm repairs missing saved states (IDLE-3)                  |
| D10 | Optional `type: final` states finish instances; no agent `finish`/abandon built-in                         |
| D11 | `yield` built-in, always offered; fixed built-in names, overridable guidance                               |
| D12 | Idle is a state; its list arrives on entry, not per prompt                                                 |
| D13 | Instance held by one session; `enter` takes over; stale holders are told "moved"                            |
| D14 | Sessions user-level; instances project-level                                                                |
| D15 | Only the main agent calls `smllm`                                                                          |
| D16 | "Machine decides" = XState guarded transitions + `always` (eventless) states; no `decide`/`done`             |
| D17 | Command input via env only; string (shell) or array (exec) `run`                                           |
| D18 | Params are not persisted on instances                                                                      |
| D19 | Format = strict XState v5 subset in YAML, smllm data under `meta` (`meta.smllm: 1`); kebab TOML; generated JSON Schema |
| D20 | Event types declared once in `meta.events`; states map events in `on`; per-state guidance = transition `description`, param prompts = `meta.paramDescriptions` |
| D21 | Agent text fenced in `<smllm>` (inner `<instructions>`, `<events>`), fixed header line incl. visit count   |
| D22 | `enter` may jump an existing instance to a `meta.entryPoint` state; recorded as a jump                     |
| D23 | Completed instances can be reopened with an explicit `state`; visits + history carry on                    |
| D24 | Three crates: `smllm-core` (no_std, WASM rules, engine only), `smllm-format` (std, parsing/validation), app; browser hosts load compiled JSON |
| D25 | `smllm-wasm` + npm `smllm-wasm` exist from P0 so the core can't drift from wasm-compatibility              |
| D26 | Project + user configs are combined; project wins on `id` clash                                             |
| D27 | Git evidence and the `history` block are cut; commands in actions (or instructions) drive git when needed  |
| D28 | XState actions: `entry`/`exit`/transition `actions` hold `prompt` and `command` actions; guards decide, actions never block; `promptInjections` → `meta.sharedActions` |
| D29 | Agent rules live in the MCP tool description (full) and a tiny `AGENTS.md`/`CLAUDE.md` block (orientation), placed by sokf's rules |
| D30 | Tests: core snapshots/properties, scripted fake-agent sessions, wasm smoke; live-model test only on human request |
| D31 | Shared code with sokf lives in `agent-harness-kit` (this repo, published, no smllm deps), factored from `submodules/sokf` |
| D32 | Params declared as a JSON Schema object subset; strings only in v1 (`enum`, `pattern`, `required`)         |
| D33 | Self-transitions follow XState (`reenter: true` to re-enter); visits count entries; stop blocked unless yielded |
| D34 | Ship a Claude Code plugin in addition to `harness install` parts                                           |

## 14. Open questions

| #   | Question                                                                                                    | Proposed default                                   |
| --- | ----------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| O3  | Which harness after Claude Code (pi, headless `smllm run`, …)?                                              | Decide after P5                                    |
| O6  | Share instances (commit `state/`)?                                                                           | Private in v1                                      |

## 15. Implementation status (2026-09-25)

| Item | State |
| ---- | ----- |
| Deps (§12) | Confirmed: serde-saphyr, toml/toml_edit, schemars, wasm-bindgen, etcetera, getrandom, wait-timeout, proptest, regex (added), rmcp + tokio (user chose rmcp); file lock = std `File::lock`; small JSON = serde_json (258 KiB wasm) |
| R5 | GitHub repo rename + local dir rename: user action; devcontainer volume names kept (renaming loses volumes) |
| NFR-8 | wasm budget 300 KiB, enforced by `scripts/build-wasm.mjs` |
| HOST-9 | Done 2026-09-25 on Claude Code 2.1.282 (`claude -p`, haiku): SessionStart `additionalContext` reaches the agent; UserPromptSubmit and Stop carry the same `session_id`; `--resume` keeps `session_id` (SessionStart `source: resume`) so the same smllm key rebinds; the MCP tool works with the installed permission; Stop blocks with the events list and the agent fires `yield`; the second Stop has `stop_hook_active: true` |
| TEST-4 | `scripts/live-e2e.mjs` run once on request 2026-09-25 (haiku): enter → written → DONE via the CHECK guard, OK. Still human-request only (`SMLLM_LIVE=1`) |
| TURN-6 | `stop_hook_active` only; sokf hash fallback deferred until a harness without it |
| Deferred | Published `--json` output schemas; `agent-harness-kit` `query` module (needs jmespath) |

