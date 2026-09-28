# PLAN-008: Fast hosts: engine scaling, a fast CLI path, and a reference JS host

| Meta               | Value                                                              |
| ------------------ | ------------------------------------------------------------------ |
| Status             | in-progress (decided D8-1..D8-22; implementation next)             |
| Workflow direction | top-down (requirements → design → code → example → docs)           |
| Traces to          | NFR (new latency requirement), IDLE (parked list), ENG-Host (MemoryStore), HOST-5, HOST-12, HOST-Mcp, HOST-Wasm, DEC-4..DEC-7, TEST-3, NFR-8, PLAN-006 D6-3 |

## 1. Goal

smllm's overhead per event must be negligible even for a model that answers in milliseconds
(Jev and similar), where each turn makes two or three smllm calls (`fire`, the stop check, a
prompt-submitted check). So:

- the engine scales with the number of instances (it degrades quadratically today);
- the CLI path gets fast where it can (the long-lived MCP server);
- JS hosts get the WASM engine in-process, with importable reference code for what every host
  needs, a TypeScript example, and guidance on WASM vs CLI;
- the numbers become a requirement, measured by a benchmark.

## 2. Measurements (2026-09-28, this devcontainer, `examples/dev` machines)

| Path | Per call |
| ---- | -------- |
| CLI process start (`smllm --version`) | 0.8 ms |
| CLI event (`smllm fire yield`: config load, lock, instance + history + session writes), normal disk | 4.5 ms |
| CLI read (`smllm instance list`), normal disk / the virtiofs share | 3.7 ms / 15.5 ms |
| WASM `new Engine` (parse compiled machines) | 0.12 ms |
| WASM `view` | 0.007–0.05 ms |
| WASM `enter` / `park`, 0 parked | 0.08 / 0.11 ms |
| WASM `enter` / `park`, 1,000 parked | 0.62 / 6.9 ms (idle text 36 KB) |
| WASM `enter` / `park`, 5,000 parked | 1.2 / 94 ms (idle text 172 KB) |

Causes of the growth (in `smllm-core`, so the CLI has them too):

1. The idle list sorts every parked instance with `insertion_sort_by` (quadratic on unsorted
   input), swapping whole `(Machine, Instance)` pairs.
2. The idle list prints every parked instance: unbounded agent text.
3. `MemoryStore` has no index: `instance_by_ref` and `instances_with` fall back to the trait
   defaults, which clone every instance of the machine, then search or filter.

## 3. Issues to decide

The options as drafted; §5 records what was decided and governs where they differ.

### A. Engine scaling (all hosts)

| ID | Question | Options (recommended first) |
| -- | -------- | --------------------------- |
| A-1 | Sorting without std's sort (23 KB of wasm, rust-rules) | (a) a small stable merge sort in `utils` (O(n log n), a few hundred bytes), used for the parked list and any other list that can grow; `insertion_sort_by` stays for fixed small lists. (b) sort indices by key with insertion sort (still quadratic comparisons). (c) keep parked instances in a `SmallMap` keyed by machine + label (sorted on insert: quadratic moves, cheap memmoves) |
| A-2 | The idle list's parked section is unbounded | (a) list at most N (e.g. 10) parked instances, then "and K more: enter one by its ref" (the agent can still enter any by ref; `smllm instance list` shows all); which N come first is decided with it (most recently updated first, or ref order). (b) keep listing all, only faster. (c) a paging event |
| A-3 | `MemoryStore` scans and clones | (a) override `instances_with` and `instance_by_ref`: filter before cloning, and keep a ref index (`machine/ref → id`), as the file store keeps markers; covered by the store contract tests. (b) filter before cloning only |
| A-4 | Other per-call costs in the engine | (a) profile `fire` / `stop` / `view` at 0, 100 and 1,000 instances after A-1..A-3 (clones of sessions and instances, text building) and fix what shows; (b) stop at A-1..A-3 |

### B. A fast CLI path

| ID | Question | Options (recommended first) |
| -- | -------- | --------------------------- |
| B-1 | The MCP server reloads the config on every call (HOST-Runtime builds a fresh engine per call) | (a) the server keeps the loaded config and reloads when any loaded file's modification time or size changes (a `stat` per file per call); prompt files are already read at request time. (b) a file watcher (a new dependency). (c) keep reloading |
| B-2 | Measuring the MCP path | (a) a benchmark that drives `smllm mcp` over stdio (JSON-RPC) and reports per-call time, before and after B-1. (b) estimate |
| B-3 | The file store's per-event I/O (lock, instance write, history append, session write) | (a) measure after B-1; change only what shows (e.g. no `fsync` is done today: keep it so). (b) leave |
| B-4 | Hooks run as one process per call (Claude Code's design) | (a) leave: ~1–4 ms per hook suits Claude Code, whose model is slow; note it. (b) — |

### C. Reference JS host (importable)

| ID | Question | Options (recommended first) |
| -- | -------- | --------------------------- |
| C-1 | The agent-facing `smllm` tool (name, description with the agent rules, input schema) lives only in the CLI (`commands/mcp.rs`) | (a) move it to `smllm-core` beside `AGENT_RULES` (one source, HOST-5 / HOST-12); the MCP server and the wasm (`tool()`) both use it. (b) copy it into the package with a test that it matches. (c) JS-only |
| C-2 | Turning a tool call into `fire` / `view` | (a) `engine.callTool(args)` in the typed wrapper, answering as the MCP server does (same errors, same text), tested against it. (b) leave to each host |
| C-3 | A full Node host for millisecond models: `command` guards and actions, and file persistence | (a) ship `smllm-wasm/node` now: a command host with DEC-4..DEC-7 semantics (shell / exec, `cwd` relative to the machine file, env, timeout, the 400-character failure tail, the failure texts) and a state file (atomic `exportState`) plus history JSONL. (b) later, when a host asks. (c) never: fast Node hosts use the CLI's MCP server (B-1) |
| C-4 | The Node command host's timeout (if C-3 a): host methods are synchronous, so it uses `spawnSync`, which kills only the direct child, not the process group (DEC GROUP KILL) | (a) unix: run the command in its own group under a small supervisor (`setsid` + `kill -- -pgid` on expiry), Windows: the child only, as the CLI; a test that a grandchild dies. (b) `spawnSync` timeout only, documented. (c) async host methods (an async engine API: out of scope) |
| C-5 | History for JS hosts | (a) the `history` callback stays the one mechanism; the Node helper (C-3) appends JSONL in the CLI's format; browsers: the example keeps it in memory, the README says where it could go (IndexedDB, a server). (b) a storage interface in the package |
| C-6 | Guidance | (a) a "WASM or CLI?" section in the package README with §2's numbers: WASM for browsers, workers and serverless, JS-defined guards and actions or storage, and millisecond models; the CLI (npm `smllm`, `smllm mcp`) for a Node app that wants its files, locking and commands as they are |

### D. TypeScript and the example

| ID | Question | Options (recommended first) |
| -- | -------- | --------------------------- |
| D-1 | The package's JS in TypeScript or hand-written with `.d.ts`? | (a) TypeScript sources compiled by `tsc` in `build:wasm`; `wrap.js` / `types.d.ts` move to TS, so declarations are generated, never hand-kept (revisits PLAN-006 D6-3). (b) hand-written as D6-3, checked by `tsc` through the example |
| D-2 | Checking TypeScript in CI | (a) `typescript` as a dev dependency (new: needs approval); `tsc --noEmit` over the package and the example in the wasm job. (b) run only |
| D-3 | The example | (a) `examples/wasm/`: an agent loop with a scripted fake model that answers instantly (a stand-in for a millisecond model): bind, read, fire, the stop check blocked then allowed after `yield`, state saved and restored, history kept, a malformed-input error; it prints smllm's overhead per turn. A runtime-neutral core (runs in a browser) plus a Node entry using `smllm-wasm/node` (if C-3 a). Run by CI under Node's type stripping. (b) also a real-LLM variant behind an env var (like `live-e2e.mjs`). (c) a browser page |

### E. A latency requirement

| ID | Question | Options (recommended first) |
| -- | -------- | --------------------------- |
| E-1 | Make the numbers a requirement | (a) NFR: engine per-event time with 1,000 instances ≤ 1 ms in-process (WASM), MCP call ≤ 2 ms, idle text bounded (A-2); `npm run bench` measures them (wasm, MCP, CLI) and prints a table; not a CI gate (shared runners are noisy), except a scaling check: time at 1,000 instances ≤ 5× time at 100 (catches quadratic regressions, robust to noise). (b) bench only, no requirement. (c) hard CI time limits |

## 4. Phases

Each phase is one commit with `npm run -s test:gate` (and `build:wasm` / `test:wasm` where the
wasm or package is touched) passing, and updates the specs it touches. The wasm stays within its
134 KiB budget (116.0 KiB now); each phase that adds wasm code reports its size.

| Phase | Content | Proof |
| ----- | ------- | ----- |
| F1 | Requirements and designs in the new terms (D8-20) | — |
| F2 | The renames in code, agent text, status line, docs (D8-4) | gate; snapshots; wasm tests; no `park` / `parked` / `suspended` left outside plans and the changelog's past entries |
| F3 | `npm run bench` (D8-1, D8-9): wasm, MCP and CLI per-call times at 0 / 100 / 1,000 instances | the baseline table in §6 |
| F4 | Merge sort (`utils::merge_sort_by`; insertion sort gone); `Store::count` / `recent` (defaults scan); `MemoryStore` private, `from_parts`, indexes by ref and by status keyed `updated` then label (count = run length, recent = newest groups, no sort, however many tie), the file store's ref rule; the file store's shelf per status, `count` by listing, `recent` by file time (ties read); idle list capped at 10 with `…and K more`, `listPaused` (a view); `newest_first` orders by time, machine, label (one sort for the merge). D8-7 profiling found `WasmStore` never delegated `instances_with` / `instance_by_ref` (since PLAN-005) nor the new two, so the wasm scanned and sorted every instance: all delegate now (rust-rules: a wrapping store delegates every method). Tests: ENG_P-5 proptest (mutation-checked twice), idle cap + `listPaused` snapshot, file store `count` / `recent` with tied times, `scaling.test.mjs` (fails on the original bug). 199 tests + 7 wasm; wasm 127,774 bytes. Local state moved to the new shelves by hand, binary reinstalled |
| F5 | `smllm-format`: `Loaded::inputs`, every file a load read or checked (configs, machine files, prompt files, found or not; `Files` records its probes). App: `cache.rs` `ConfigCache` (an entry per set of config files; each input's time, size and bytes, or missing; racy rule with a 2 s margin), `Runtime::call_configs`, the MCP server keeps it under its call lock. Tests: a same-size edit within a tick, a missing machine file that appears, reuse when old (mutation-checked: without the racy check it serves the stale load); a load lists a missing prompt file. MCP 100 paused: enter 0.42 → 0.37 ms, view 0.20 → 0.16 ms (the bench's config is tiny) |
| F6 | `smllm-core` `tool.rs`: `TOOL_NAME`, `tool_description()`, `TOOL_INPUT_SCHEMA` (moved from the MCP server, text unchanged: its snapshot passes); `Engine::call` (view without an event, needs a key; else fire); the MCP server and `Runtime::call` use them. Wasm `tool()` and `callTool()` (the MCP server's argument rules and texts, errors as `ok: false` replies); wrapper and types. One call table (`packages/smllm-wasm/test/tool-calls.json`) answered word for word by the MCP server (session.rs) and the wasm (smoke test). Wasm 132,216 bytes (+4.4 KB: the agent rules text now ships in the wasm) |
| F7 | `typescript` 7.0.2 dev dependency; `packages/smllm-wasm/src/` (`types.ts` moved from `types.d.ts`, `wrap.ts` a named `Engine` base class, `index.ts` / `bundler.ts` subclass it with their build's raw engine as a static, so `Engine` is a class and a type); `tsconfig.json` (strict, `noUncheckedIndexedAccess`) compiles to `dist/`, which the exports and `files` point at; `build:wasm` runs `tsc` (type errors fail it, in CI too); hand-kept `wrap.js`, `index.js`, `bundler.js` and their `.d.ts` removed. A consumer check: correct use compiles, a number param and `text` on `allow` are errors. 8 wasm tests |
| F8 | Wasm store writes through: `put(kind, key, record)` for each saved session, binding and instance (a throwing `put` loses the record, never the transition). TS: `Host.put`, `Storage`, `State` typed, the `Engine` constructor's `storage` (loaded once, then every write), `memoryStorage()`, and `smllm-wasm/node` with `nodeFileStorage(dir)` (`{key, record}` per file, atomic, `fileName` escapes all but `[a-z0-9-]` and hashes long names; history JSONL); tsconfig names `node` types (TS 7 does not pick them up). Tests: a restored engine equals the original (memory and files), writes per cycle equal at 100 and 1,000 instances (≤ 6), a throwing `put`, file names. 13 wasm tests; wasm 133,132 bytes |
| F9 | `compile` writes a relative `cwd` relative to the compiled file, `/`-joined on every OS, `.` dropped (`Files::with_prefix` from `[machines] files` as written; a single machine file compiles as before). `smllm-wasm/node` `nodeHost({ configDir, timeoutSecs? })`: DEC-4..DEC-7 as the CLI's runner (shell / exec, env, `cwd` against `configDir` else the session's, 60 s default, 400-character tail, the failure texts); `spawnSync` with `detached` and a group kill on timeout instead of D8-15's `sh` supervisor (see its amendment). Tests: compiled `cwd` (`machines/build`, `.`, absolute); the CLI runner's cases in Node (shell, exec, env, exit + tail, a megabyte of output, missing run, a leftover process not holding the call, `cwd`, a timed-out grandchild killed and a bystander alive: mutation-checked). Windows paths untested here |
| F10 | `examples/wasm/`: `note.smllm.yaml` (a command guard checks the note, an action files it; `machines.json` regenerated by `build:wasm`), `agent-loop.ts` (web-standard APIs only: `tool()`, `callTool()`, `stop()`, a scripted instant model), `node.ts` (`nodeHost`, `nodeFileStorage`; two runs on one folder, the second a new engine continuing the paused note; asserts the guard's `→ false`, the blocked stop, `visit 3`, the filing, the malformed call's answer, the filed text; prints smllm's time apart from storage and commands: warm ~0.15 ms per turn, cold ~1.2 ms as V8 compiles the wasm). `npm run test:example` (tsc then node), a CI step in the wasm job, CONTRIBUTING |
| F4 | Engine scaling: merge sort, idle cap and `listPaused`, `MemoryStore` indexes, `Store::count` / `recent`, the file store's shelf per status (D8-2, D8-3, D8-5, D8-6, D8-21), then D8-7's findings | the scaling check in CI; the proptest; store contract tests; idle snapshots; bench |
| F5 | MCP config cache (D8-8; D8-10 if it shows) | an edited machine file is picked up on the next call, including within one timestamp tick; bench |
| F6 | The tool definition in core; `Engine::call`; `tool()` / `callTool()` (D8-12, D8-13) | the MCP server serves core's tool; one call table answered alike by MCP and wasm |
| F7 | TypeScript toolchain (D8-18) | smoke tests unchanged; `tsc --noEmit` clean |
| F8 | Write-through storage: the wasm store forwards `put`; `Storage`, `memoryStorage`, `nodeFileStorage` (D8-16) | a stored engine restored from its folder equals the original; per-call cost flat from 100 to 1,000 instances |
| F9 | `compile` paths relative to the config (D8-22); `smllm-wasm/node`: `nodeHost` and its supervisor (D8-14, D8-15) | Node tests mirroring the CLI runner's (shell, exec, env, cwd, exit + tail, timeout incl. a grandchild, spawn failure, missing run) |
| F10 | `examples/wasm/` (D8-19), run and type-checked in CI | CI runs it; it prints the per-turn overhead |
| F11 | README "WASM or CLI?" with the final bench table (D8-17), ARCHITECTURE, CHANGELOG, §6 | — |

## 5. Decisions

- D8-1 (E-1): targets as (a): in-process (wasm) ≤ 1 ms per event with 1,000 instances, MCP call
  ≤ 2 ms (with 100 paused and 1,000 completed instances, on local disk), idle text bounded; `npm run bench` prints them; CI enforces only the scaling check
  (time at 1,000 instances ≤ 5× time at 100).
- D8-2 (A-2): the idle list shows at most 10 paused instances, most recently updated first, then
  "…and K more paused: fire listPaused to list them all". A realistic maximum is ~100.
- D8-3 (A-2): a new read-only built-in idle event `listPaused` lists every paused instance, most
  recent first, offered only when the idle list was cut short; no params; it changes nothing (no
  history, no version bump). The name is reserved like the other built-ins (CFG-13).
- D8-4 (terms): rename for clarity, as the first phase, nothing released so no compatibility:
  `park` → `pause` (event), `parked` → `paused` (status); `suspended` → `interrupted` (status,
  the session's `suspended` field, the status line's JSON key and text). Interrupted = a detour
  by `unmatched`, back with `resume` (the fallback-state path already records the `interrupted`
  state); paused = stopped for later, back with `enter` and the ref.
  The instance's own `interrupted` field (the state a fallback detour returns to) becomes
  `resumeState`, so "interrupted" means one thing (F2).
- D8-5 (A-1): one stable merge sort in `utils` (O(n log n), a scratch buffer) replaces
  `insertion_sort_by`, whose only use is the paused list: sort all paused by `updated` (most
  recent first), ties by machine then label; idle takes the first 10, `listPaused` all. The
  rust-rules sort rule points to it. Measure its wasm cost.
- D8-6 (A-3): `MemoryStore` done right: private fields, one constructor from saved state that
  builds the indexes (used by serde, smllm-json and `importState`), read accessors for tests;
  derived, never serialized indexes by ref (machine + ref → id) and by status (machine + status
  → ids); `put_instance` the one write path, updating both; a ref held by another instance of
  the machine is a `Conflict`, as in the file store; a proptest runs random operation sequences
  and checks the indexed lookups equal a plain scan after every step.
- D8-7 (A-4): after A-1..A-3, profile `fire` / `stop` / `view` at 0, 100 and 1,000 instances
  with the bench and fix what takes a meaningful share of the 1 ms target.
- D8-8 (B-1): the MCP server caches the loaded config, keyed by the discovered config files,
  and checks it each call like git's racy-clean rule: a file whose modification time or size
  changed, or whose modification time is not clearly older than the cache, is re-read and
  compared; any difference rebuilds; the rest are trusted. Exact, and read-free after a tick.
- D8-9 (B-2): the bench drives a real `smllm mcp` over stdio (JSON-RPC) on a copy of
  `examples/dev` on local disk, before and after D8-8.
- D8-10 (B-3): measure the file store's per-event I/O after D8-8; change only what shows; no
  `fsync`.
- D8-11 (B-4): hooks stay one process per call; the guidance says millisecond-model harnesses
  use MCP or the wasm.
- D8-12 (C-1): the tool definition moves to `smllm-core` beside `AGENT_RULES`: `TOOL_NAME`,
  `tool_description()`, `TOOL_INPUT_SCHEMA` (a JSON string); the MCP server parses it once (a
  test checks it serves core's); the wasm exposes `tool()` → `{ name, description, inputSchema }`.
- D8-13 (C-2): `Engine::call(host, session, event, params, bind)` in core does view-or-fire;
  the MCP server and the wasm both use it; each parses tool arguments with the same error texts
  (one shared table of calls and answers tests both). The wrapper's
  `callTool({ session?, event?, params? })` returns the full reply; bad arguments return
  `{ ok: false, text: "error: …" }`, never throw.
- D8-14 (C-3): ship `smllm-wasm/node` now: `nodeHost({ configDir, timeoutSecs? })` (after
  D8-22; was `machineDir`), a command host with DEC-4..DEC-7 semantics (`cwd` relative to
  `configDir`, else the session's `cwd`).
- D8-15 (C-4): unix: `spawnSync("sh")` runs a small POSIX supervisor (`set -m`: the command in
  its own process group, a background timer that kills the group on expiry, the timer stopped
  as soon as the command is reaped, the command's status passed through); Windows:
  `spawnSync`'s timeout, the child only, as the CLI; ~1 ms per command; a test that a grandchild
  dies on timeout.
  Amended in F9: dash (Debian's and Ubuntu's `/bin/sh`) turns job control off without a terminal
  ("can't access tty") and its `kill` rejects `--`, so `set -m` gives no process group there.
  `spawnSync` honours `detached: true` (setsid; measured: the child leads its own session and
  group), so the host starts the command detached, lets `spawnSync` time it out, then kills the
  rest of its group; output goes to a file so a leftover process cannot hold the call. No
  supervisor process at all, and no extra cost.
- D8-16 (C-5): write-through storage, not snapshots (`exportState` measured 0.91 ms and 210 KB
  per call at 1,000 instances): the wasm store keeps `MemoryStore` as its working copy and hands
  each saved record to the host (`put(kind, key, record)` for sessions, bindings, instances;
  `history` as since D6-4). The package's `Storage` interface: `load()` (once, into
  `importState`), `put`, `history`; shipped `memoryStorage()` and `nodeFileStorage(dir)` (a file
  per record, atomic; history JSONL in the CLI's line format; one process per folder); browsers
  implement it (the example shows IndexedDB's shape). `exportState` stays for backups and moves.
  Not the CLI's file layout. Extends PLAN-006 D6-4.
- D8-21 (DC-5): a shelf per status in the file store: `active/`, `interrupted/`, `paused/`,
  `done/` (was `open/` + `done/`; nothing released, no migration). `Store` gains
  `count(machine, status)` and `recent(machine, status, limit)`, whose defaults filter a scan.
  The file store counts by listing a shelf (no reads; the status line too) and serves `recent`
  from a listing with file times, reading only the newest `limit` files, then ordering them by
  `updated`; `MemoryStore` serves both from D8-6's indexes. The idle list takes each machine's
  10 most recent, merges, keeps 10; only `listPaused` reads all.
- D8-22 (DC-6): `compile` rewrites a relative command `cwd` to be relative to the compiled
  config file (`cwd: build` in `machines/dev.yaml` → `machines/build`; absolute stays); still
  the same bytes from any folder (D6-2's goal). Hosts need one base folder: the config's.
- D8-17 (C-6): a "WASM or CLI?" section in the package README with the bench's table: wasm for
  browsers, workers, serverless, millisecond models, JS guards / actions / storage; the CLI (npm
  `smllm`, `smllm mcp`) for shared files with locking, the status line, `instance list`, hooks.
- D8-18 (D-1, D-2): the package's JS becomes TypeScript (`src/*.ts`, `wrap.js` / `types.d.ts`
  included), compiled by `tsc` in `build:wasm` to `.js` + generated `.d.ts`; `typescript` is a
  dev dependency (approved); CI's wasm job runs `tsc --noEmit` over the package and the example.
  Reverses PLAN-006 D6-3 (hand-kept declarations do not scale to this much code).
- D8-19 (D-3): `examples/wasm/`: a runtime-neutral `agent-loop.ts` (web-standard APIs only; a
  scripted instant fake model given `tool()`, calls through `callTool()`, the stop check blocked
  then allowed after `yield`, state through `Storage`, history kept, a malformed-input error,
  smllm's overhead per turn printed) and `node.ts` (the same loop with `nodeHost` and
  `nodeFileStorage`, run twice so the second continues from stored state); CI runs it under
  Node's type stripping and type-checks it. No real-LLM variant or browser page for now.
- D8-20 (requirements): REQ-NFR (the latency requirement, D8-1), REQ-IDLE (D8-2, D8-3, the new
  terms), REQ-INST / REQ-TURN / REQ-STL / REQ-CFG (the renames; reserved names include `pause` and
  `listPaused`), REQ-HOST (a JS-host requirement: D8-12..D8-16), REQ-TEST (TEST-3: the example and
  the type check); designs ENG, IDLE, HOST, NFR, STO follow. Written first, in the new terms.

### 5.1 Double-check (2026-09-28)

Resolved in the plan:

- DC-1: `listPaused` is a view for the stop logic: it writes nothing to the session either, so
  it does not count as an event fired since a stop block (the runaway rule, PLAN-003); the
  agent that only lists and stops again is let go, as after a view.
- DC-2: the scaling check covers `fire`, `enter`, `pause`, `stop` and `view` at 100 vs 1,000
  instances; not `listPaused`, whose output is linear in the paused count by design.
- DC-3: D8-8's racy check allows for the file system's clock differing from the process's
  (a network or VM share): "not clearly older" means older than the cache's build time minus a
  2-second margin.
- DC-4: `nodeFileStorage` names each record's file from its key the way the file store names
  ref markers (lowercase-safe, a hash suffix when needed): binding keys hold arbitrary host
  session ids, and case-insensitive file systems must not merge two keys.

Found, then decided:

- DC-5 → D8-21: the file store's paused list read every `open/` instance file on each idle view
  (and the status line's count did too): O(open instances) file reads per idle view through the
  CLI and MCP, against D8-1's 2 ms. D8-6 fixed `MemoryStore` only.
- DC-6 → D8-22: a compiled command's `cwd` was relative to its own machine file (D6-2), but
  compiled output does not record each machine's folder, so D8-14's single `machineDir` was
  wrong for a config whose machine files sit in different folders.

## 6. Outcome

| Phase | Commit / result |
| ----- | --------------- |
| F1 | Requirements: IDLE-7 (bounded idle list, `listPaused`), NFR-10 (per-event latency), HOST-13..HOST-16, TEST-3_AC-2, DEC-7 (compile `cwd`); the terms pause / interrupted across 15 specs. Designs: ENG (BUILTINS, `Store::count` / `recent`, `MemoryStore` indexes, ENG_P-5, merge sort, tool definition, `Engine::call`), IDLE, TURN (layout), STO (a shelf per status), HOST (MCP cache, write-through wasm, HOST-JsPackage), NFR (bench, scaling check), CFG / DEC (compile `cwd`), STL |
| F2 | The terms in code, agent text, status line, snapshots, docs, skills, examples and the unreleased CHANGELOG (34 files); `Instance.interrupted` → `resume_state` (`resumeState`). 194 tests, 6 wasm tests; wasm 118,777 bytes. This repo's local state converted by hand (4 instance files, 19 sessions; no migration code, D6-1) and the binary reinstalled |
| F3 | `npm run bench` (`scripts/bench.mjs`, machine in `scripts/bench/`: no commands; a temp copy with its own XDG dirs; release CLI). Baseline in §6.1 |

### 6.1 Bench (median ms per call; store size = paused / completed; local disk, this devcontainer)

| path | call | F3 0 / 0 | F3 100 / 1000 | F3 1000 / 1000 | F4 0 / 0 | F4 100 / 1000 | F4 1000 / 1000 |
| ---- | ---- | ---: | ---: | ---: | ---: | ---: | ---: |
| wasm | enter | 0.052 | 0.233 | 0.428 | 0.039 | 0.017 | 0.024 |
| wasm | view | 0.019 | 0.007 | 0.009 | 0.017 | 0.007 | 0.009 |
| wasm | stop | 0.015 | 0.006 | 0.007 | 0.014 | 0.006 | 0.007 |
| wasm | pause | 0.054 | 0.331 | 4.312 | 0.063 | 0.027 | 0.032 |
| mcp | enter | 0.531 | 0.493 | 0.786 | 0.439 | 0.418 | 0.447 |
| mcp | view | 0.232 | 0.204 | 0.295 | 0.226 | 0.199 | 0.203 |
| mcp | pause | 0.468 | 1.120 | 11.781 | 0.447 | 0.615 | 2.348 |
| cli | fire enter | 1.448 | 1.448 | 1.939 | 1.469 | 1.692 | 1.660 |
| cli | session show | 1.192 | 1.168 | 1.438 | 1.381 | 1.325 | 1.212 |
| cli | statusline | 1.259 | 1.939 | 7.906 | 1.246 | 1.325 | 1.425 |
| cli | fire pause | 1.426 | 2.232 | 13.504 | 1.515 | 1.898 | 4.493 |

F3 findings: the release MCP server is already 0.2–0.8 ms per call, and a CLI call ~1.4 ms (§2's 4.5 ms
`fire yield` was the installed binary on the container's overlay file system; the cause of the gap is not
established); the growth is where DC-5 said: `pause` (the idle list reads every
paused instance) and the status line's count (reads every open file); wasm `enter` grows with all instances
(`instance_by_ref` clones them, D8-6).

F4: the wasm is flat and ~30–140× faster at 1,000 paused; MCP `pause` meets D8-1 at 100 paused (0.62 ms); at
1,000 paused the file store's `recent` still stats every `paused/` file (2.3 ms), outside D8-1's stated size.
