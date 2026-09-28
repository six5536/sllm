# PLAN-006: Idiom review of storage, compile and the WASM host

| Meta               | Value                                                        |
| ------------------ | ------------------------------------------------------------ |
| Status             | completed (C1–C7, 2026-09-28)                                |
| Workflow direction | bottom-up (review → decisions → code → specs)                |
| Traces to          | STL-3, STO-1, STO-2, CLI-3, CLI-13, CFG (cwd), DEC-7, NFR-8, HOST-Wasm, INST-9 |

## 1. Goal

Settle the issues found reviewing PLAN-005's storage and the WASM host for idiom, one by one,
then implement what is decided. Items are ordered by severity.

## 2. Issues

| ID | Issue | Options (recommended first) | Traces |
| -- | ----- | --------------------------- | ------ |
| I-1 | **Read-only commands write.** PLAN-005's migration runs on first access, so the first status line after an upgrade takes the lock and moves files, breaking STL-3_AC-1 (MUST: no store writes while answering `statusline`); `validate`, `instance list/show` and `session show` also write | (a) readers read both layouts (shelves, else the old `<machine>/<id>.json`) without writing; only `put_instance` / `append_history`, already under the lock, migrate. (b) an explicit `smllm migrate` command, readers read both. (c) accept the one-time write and relax STL-3 | STL-3, STO-1 |
| I-2 | **`compile` bakes the builder's paths.** `params.cwd` is made absolute on the machine that ran `compile`, so the artifact is not reproducible and is wrong anywhere else (including the browser) | (a) compiled output keeps `cwd` as written (relative to the machine file); the CLI resolves it at load. (b) drop `cwd` from compiled output. (c) keep | CLI-13, DEC-7 |
| I-3 | **Stringly-typed WASM API.** `fire(key, event, paramsJson)` and every reply are JSON strings; host callbacks get params and env as JSON strings | (a) a small typed JS/TS wrapper in `packages/smllm-wasm` that parses, the raw string API underneath (no size cost). (b) JS objects through `serde-wasm-bindgen` plus generated TS types (size cost, to measure against the 300 KiB budget; 257 KiB now). (c) keep, document | NFR-8, HOST-Wasm |
| I-4 | **WASM state grows without bound.** `exportState` / `importState` move one snapshot holding every completed instance and all history forever (INST-9) | (a) keep the snapshot; add a host-side prune (e.g. `pruneCompleted(olderThan)` or export without completed) and document it. (b) a JS-implemented `Store` (sync only, so `Map` / `localStorage`, not IndexedDB). (c) keep, document | INST-9, HOST-Wasm |
| I-5 | **`--config` replaces rather than layers.** An explicit config drops both user and project configs; git/npm layer instead | (a) keep, as documented (explicit = only this one; predictable in CI and tests). (b) layer: user, project, then explicit | CLI-3, D26 |
| I-6 | **Instance state cannot be moved.** Always `state/` beside the listing `config.toml` | (a) `[state] dir = "…"` in `config.toml`, relative to it, default `state`; validate that two configs do not share a dir for one machine id. (b) an `SMLLM_STATE_DIR` env var as well. (c) keep (YAGNI) | STO-1 |
| I-7 | **A corrupt ref marker is silent until used.** `enter` with that ref errors (naming the file), but `validate` does not report it | (a) `validate` reports unreadable markers as STO-1 warnings, like unreadable instances. (b) rebuild markers from the instances on a read error. (c) keep | STO-1, INST-3 |

Not issues (checked): XDG paths on macOS rather than `~/Library` (usual for CLIs); walking up to
the nearest `.smllm/` (like `.git`); the auto-created `state/.gitignore`; `flock`-style locking.

## 3. Phases

Each phase is one commit with `npm run -s test:gate` passing, and updates the specs it touches.

| Phase | Decision | Proof |
| ----- | -------- | ----- |
| C1 | D6-1: delete the migration | store tests without the migration test; strace: `statusline` on a fresh machine dir writes nothing |
| C2 | D6-7: marker slow path, rebuild, `validate` warning | store test: a corrupt marker still resolves, is rebuilt by a write, and is reported |
| C3 | D6-6: `[state] dir` | load test (relative, absolute, default); `validate` error for a shared dir; session test with a moved dir |
| C4 | D6-2: `compile` keeps `cwd` as written | test: one machine with a `cwd` compiled from two folders is identical; the CLI still resolves it |
| C5 | D6-4: WASM history to the host | smoke test: `exportState` has no history; the host callback gets each entry; wasm size |
| C6 | D6-3: typed wrapper, `smllm-wasm/raw` | smoke test through the wrapper and one raw call; `.d.ts` matches the wrapper |
| C7 | Docs and outcome: CHANGELOG, ARCHITECTURE, README (package), §5 | — |

## 4. Decisions

- D6-1 (I-1): nothing is released, so no compatibility: delete the migration. Readers read only
  the shelves (`open/`, `done/`, `refs/`, `history/`; a missing folder is no instances) and never
  write; writers create the folders they need under the lock they already take. The CHANGELOG's
  "moves on first use; restart after upgrading" note goes. The same rule (no compatibility code
  before the first release) applies to every later item.
- D6-2 (I-2): compiled output is the same bytes on any machine, from any folder. It holds no
  path of the machine that compiled it: a command's `cwd` stays as the author wrote it
  (relative to the machine file), and the CLI resolves it when it loads for running. Checked:
  `cwd` is the only location-dependent value today (prompts are inlined). Test: one machine
  with a `cwd`, compiled from two folders, gives identical output.
- D6-3 (I-3): a typed wrapper over the string core, in `packages/smllm-wasm`: hand-written
  `index.js` + `index.d.ts` (no TypeScript build, so no new dependency), for both the `web` and
  `bundler` builds. The package's main export is the wrapper: `Engine` methods take and return
  objects (`fire(key, event, params)` → `Reply`, `stop()` → `{decision, text?}`, `status()` →
  `SessionStatus`, `exportState()` → object), and it adapts an object-based host (`check`,
  `run` get `params` and `env` as objects). The string API stays as `smllm-wasm/raw`. The wasm
  does not change size. The smoke test runs through the wrapper, plus one raw call.
- D6-4 (I-4): state and log apart, as in the CLI and event-sourced systems. The engine never
  reads history (only the CLI's `instance show` does), so the WASM host keeps none: a WASM store
  wrapping `MemoryStore` forwards each entry to a new host callback `history(machine, id, entry)`
  (in the typed wrapper optional; without it, entries are dropped). `exportState` / `importState`
  carry sessions, bindings and instances only. The host owns persistence and retention, as with
  XState's persisted snapshots; no prune API (YAGNI). The core and its `Store` trait do not
  change; `MemoryStore` keeps history for the core's tests.
- D6-5 (I-5): keep: no change. A path flag replaces discovery (ESLint `-c`, Ruff/Black and
  Prettier `--config`, `STARSHIP_CONFIG`); layering is the idiom for key/value overrides
  (`git -c`, `cargo --config`), which smllm does not have. One level is replaced through
  `XDG_CONFIG_HOME` (user) and the working directory (project).
- D6-6 (I-6): now. `[state] dir = "…"` in `config.toml` (the file's style, like
  `[machines] files`), relative to that file or absolute, default `state`; each machine of the
  file keeps its instances in `<dir>/<machine>/`, and `state/.gitignore` is written in `<dir>`
  as today. No environment variable: it would be global, and a user and a project machine of
  one id would share state (mypy / Ruff / pytest / Terraform checked: a config setting is the
  common form). `validate` errors when two loaded configs give one machine id the same dir.
  STO-1 gains the setting.
- D6-7 (I-7): markers are an index derived from the instances (each holds its ref), so their
  corruption is a slow path, never an error. A reader that cannot read a marker scans the
  machine's instances for the ref (still no writes, STL-3); `put_instance`, under the lock,
  rebuilds that marker from the scan; `validate` warns about an unreadable marker (STO-1), as
  for instance files. A missing marker still means no such ref (markers are written before
  their instance, under the lock): scanning then would bring back the scan on every new ref.

## 5. Outcome

| Phase | Commit / result |
| ----- | --------------- |
| C1 | 01e87be: migration deleted; reads write nothing (a test reads a machine with no state and checks nothing was created); writes create their folders |
| C2 | f2c7e92: an unreadable marker: lookups scan, a write rebuilds it (test: `R9` still resolves, setting `r9` rebuilds the bucket with both), `validate` warns (e2e) |
| C3 | 2c2bb05: `[state] dir` (relative, absolute, default tested; e2e: `enter` writes to the moved dir and its `.gitignore`). D6-6's "shared dir" `validate` error dropped: within one load a machine id appears once (a project machine replaces the user's), so it cannot happen; two projects sharing a dir is the use case, not an error |
| C4 | 0a976c4: `compile` keeps `cwd` as written; one machine compiled from two folders is byte-identical; a run load still resolves it |
| C5 | 2f57355: `WasmStore` hands history to `host.history`; `exportState` has no history; a throwing `history` keeps the transition. Wasm 257.0 KiB |
| C6 | c85212e: `wrap.js`, `index.js`, `bundler.js`, `types.d.ts`; the main export is typed, `./raw` and `./raw/bundler` the string API; host methods optional with defaults. Smoke tests through the wrapper (5). Types checked once with `npx typescript@5` (a good consumer compiles; a number param and `text` on `allow` are errors); wasm-bindgen's own `.d.ts` needs the `esnext.disposable` lib, as before |
| C7 | CHANGELOG, ARCHITECTURE 0.6.0, this section |
