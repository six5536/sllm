# Design Specification

## Overview

Implements REQ-HOST. Every surface (Claude Code hooks, the MCP server, `smllm fire`, the wasm `Engine`) builds a `Host` and calls the core protocol in `smllm-core` (ENG-Engine). In the binary, `HOST-Runtime` does the wiring per call: load configs (by lookup for a new session, or the ones recorded on an existing session), build the engine and an `FsStore`, and supply the command host. `HOST-Claude` adapts Claude Code's hook JSON through `agent-harness-kit` and declares smllm's harness parts as a kit `Tool`. `HOST-Mcp` is an `rmcp` stdio server with one hand-built tool. `HOST-Wasm` exposes the same protocol to JavaScript. The core parts of HOST-1..4 (protocol, key generation, bind, keyless `enter`) live in ENG-Engine and are specified with the engine.

## Architecture

AFFECTED LAYERS: smllm app (runtime, commands/harness, commands/mcp), smllm-wasm, agent-harness-kit, Claude Code plugin

### High-Level Architecture

```mermaid
flowchart LR
    SS[SessionStart] --> HC[HOST-Claude answer]
    UPS[UserPromptSubmit] --> HC
    ST[Stop] --> HC
    Tools[MCP tools/call] --> HM[HOST-Mcp]
    Fire[smllm fire] --> CC[CLI-Commands]
    HC --> RT[HOST-Runtime]
    HM --> RT
    CC --> RT
    RT --> Eng[ENG-Engine]
    RT --> FS[STO-FileStore]
    RT --> Cmd[DEC-CommandRunner]
    JS[JS host object] --> HW[HOST-Wasm] --> Eng
    HC --> Kit[agent-harness-kit hook + harness]
    Install[harness install/status] --> HC
```

### Module Organization

```
crates/app/smllm/src/
├── runtime.rs              # HOST-Runtime
└── commands/
    ├── harness.rs          # HOST-Claude: Tool impl, hook answers, install/status
    └── mcp.rs              # HOST-Mcp: rmcp ServerHandler, one tool
crates/app/smllm/src/cache.rs      # HOST-Mcp: ConfigCache (HOST-16)
crates/lib/smllm-wasm/src/lib.rs   # HOST-Wasm
packages/smllm-wasm/src/           # HOST-JsPackage (TypeScript, compiled by tsc in build:wasm)
├── wrap.ts                 # the typed Engine: objects in and out, tool(), callTool()
├── storage.ts              # Storage, memoryStorage()
└── node/                   # smllm-wasm/node: nodeHost (command runner + supervisor), nodeFileStorage
crates/lib/agent-harness-kit/src/
├── fs.rs                   # read_text, write_atomic (keeps symlinks and modes)
├── harness/                # install/status, parts, region, merge, target, record, declined
└── hook/                   # HookInput, Answer, emit, LoopGuard
plugin/                     # Claude Code plugin (hooks.json, .mcp.json, plugin.json)
.claude-plugin/marketplace.json
```

### Architectural Decisions

- RMCP OVER HAND-ROLLED MCP: user decision in P1; `rmcp` tracks protocol versions and capabilities. Cost: a current-thread `tokio` runtime used only by `smllm mcp`. Alternatives: hand-rolled JSON-RPC over stdio (no async runtime)
- ONE HAND-BUILT TOOL SCHEMA: the input schema is a literal JSON object (`params` = object of strings, CFG-16) rather than derived via rmcp macros, so it stays a stable contract; since PLAN-008 it is the core's `TOOL_INPUT_SCHEMA`, shared with the wasm (HOST-13). Alternatives: `#[tool]` macro with a schemars-derived struct
- CACHED CONFIG, CHECKED EACH CALL: the MCP server outlives its calls, so it keeps the loaded config and checks the files it came from each call (racy-clean rule); hooks stay one process per call, fine for Claude Code's slow model (PLAN-008 D8-8, D8-11). Alternatives: reload per call (~2–4 ms), a file watcher (a dependency)
- ENGINE CALLS OFF THE ASYNC RUNTIME: `spawn_blocking`, since guards and actions may run commands for minutes. Alternatives: async command runner
- ONE CALL AT A TIME: each `tools/call` holds the server's `calls` mutex, so pipelined calls on one session cannot interleave their store reads and writes (PLAN-003 F3). Alternatives: a lock per session key (more code; one agent rarely pipelines)
- SESSION-BOUND CONFIGS: `Runtime::for_session` loads the configs recorded on the session (STO-2), not the caller's lookup, so MCP calls from any cwd act on the right machines
- BINDING PER HARNESS SESSION ID: the Claude hook looks up `bindings/claude/<session_id>`; no binding → bind a new session (HOST-4). The MCP server has no harness session id, so the agent passes the key it saw in the header (D5)
- RUNAWAY GUARD FROM `stop_hook_active`: the kit's `LoopGuard` (sokf's same-text hash) is not used by the Claude hook; the core's `Stop::Runaway` covers TURN-6
- KIT'S `query` MODULE NOT PORTED: it needs `jmespath`, an unapproved dependency, and smllm does not use it. Alternatives: port behind a feature once approved
- PLUGIN IN ADDITION TO INSTALL PARTS (D34): the plugin wires hooks and MCP only; the instructions block and permissions stay with `harness install`
- USER-SCOPE MCP THROUGH THE `claude` CLI: `~/.claude.json` is Claude Code's internal state file, so it is only observed, never written

## Components and Interfaces

### HOST-Runtime

Per-call wiring. `new` loads configs with `smllm_format::load_configs`, maps each machine id to its `state_dir`, builds `Engine` and `FsStore` (user state dir), and remembers the config paths for new sessions. `lookup` resolves configs from a cwd (CLI-Lookup). `for_session` reads the session with a machine-less store and reloads its recorded configs; an unknown key yields an empty config and the engine reports the unknown session. `for_call` picks between them (a key → `for_session`, none → `lookup`); `fire` fires with a `Bind` for a keyless `enter` — the one path `smllm fire` and the MCP tool share. `with` builds a `Host` from `FsStore`, `Commands` (guards and actions), `Files` (prompt files, regex matcher, clock) and `OsIds`.

```rust
pub struct Runtime {
    pub loaded: Loaded,
    pub engine: Engine,
    pub store: FsStore,
    pub configs: Vec<String>,
}

impl Runtime {
    pub fn new(files: &[ConfigFile]) -> Result<Self>;
    pub fn lookup(explicit: Option<&Path>, cwd: &Path) -> Result<Self>;
    pub fn for_session(key: &str) -> Result<Self>;
    pub fn for_call(key: Option<&str>, explicit: Option<&Path>, cwd: &Path) -> Result<Self>;
    pub fn fire(&mut self, harness: &str, key: Option<&str>, event: &str, params: &[(String, String)], cwd: &Path) -> Result<Reply>;
    pub fn with<R>(&mut self, f: impl FnOnce(&Engine, &mut Host<'_>) -> R) -> R;
    pub fn bind(&mut self, harness: &str, host_session: Option<&str>, cwd: &str) -> Result<Reply>;
}
```

### HOST-Claude

Hooks: `hook` reads stdin into `HookInput::parse` (bad JSON = empty input), calls `answer`, and writes through the kit's `emit` (answer JSON on stdout and exit 0; failure → `error:` on stderr, empty stdout, exit 1). An unknown harness name exits 2. `answer` resolves the bound key from `bindings/claude/<session_id>`:

- `session-start`: bound and stored → `view` (entry block); else lookup configs from the input `cwd` (none → `{}`), `bind("claude", session_id, cwd)`; the reply text goes out as `hookSpecificOutput.additionalContext`
- `user-prompt-submit`: bound → `prompt_submitted` (clear yield); always `{}`
- `stop`: unbound → `{}`; else `stop(key, stop_hook_active)` → `Allow` = `{}`, `Block(text)` = `{"decision":"block","reason":text}`, `Runaway(text)` = `{}` with the list on stderr

Install/status: `Smllm` implements the kit's `Tool`; `run` parses the scope and calls `install` or `status`, printing text or `--json`. The project root is the directory holding `.smllm/` (else cwd); the user root is `~/.claude`. Parts per scope:

- `instructions`: region part with `INSTRUCTIONS_BLOCK`; the kit picks `AGENTS.md`/`CLAUDE.md` (HOST-10) and applies the `<!-- smllm:harness -->` region rules (HOST-11)
- `mcp`: project → merge `mcpServers.smllm = {command: "smllm", args: ["mcp"]}` into `.mcp.json`; user → external part `ClaudeMcpUser` (write = `claude mcp remove` then `claude mcp add-json --scope user smllm <json>`; observe = read `/mcpServers/smllm` from `~/.claude.json`). The kit writes external parts before any file, so a failing `claude mcp` leaves every file as found (NFR-6); files are written atomically (temp + rename)
- `hooks`: merge into `.claude/settings.json` / `~/.claude/settings.json`: one own group per event (`SessionStart`, `UserPromptSubmit`, `Stop`) keyed by the command prefix `smllm harness hook `
- `permissions`: add `mcp__smllm__smllm` to `permissions.allow` in the same settings file

Record: `.smllm/harness.toml` or `~/.config/smllm/harness.toml`. Declined parts (`--without`): `[harness.claude] without = [...]` in `.smllm/config.toml` or `~/.config/smllm/config.toml` via the kit's `TomlDeclined`, edited in place.

IMPLEMENTS: HOST-11_AC-1, HOST-6_AC-1, CLI-7_AC-1, HOST-7_AC-1, HOST-8_AC-1, CLI-8_AC-1

```rust
pub const INSTRUCTIONS_BLOCK: &str;

pub struct Smllm { /* project, claude_user, user_config: PathBuf */ }
impl Smllm { pub fn new(cwd: &Path) -> Result<Self>; }
impl agent_harness_kit::Tool for Smllm {
    fn name(&self) -> &str;                                   // "smllm"
    fn profile(&self, harness: &str, scope: Scope) -> Option<Profile>; // "claude" only
    fn root(&self, scope: Scope) -> agent_harness_kit::Result<PathBuf>;
    fn record_path(&self, scope: Scope) -> agent_harness_kit::Result<PathBuf>;
    fn declined_store(&self, scope: Scope) -> agent_harness_kit::Result<Box<dyn DeclinedStore + '_>>;
}

pub fn run(args: &HarnessArgs, installing: bool) -> Result<u8>;
pub fn answer(hook: &str, input: &HookInput) -> Result<Answer>;
pub fn hook(args: &HookArgs) -> Result<u8>;
```

### HOST-Mcp

`serve` builds a current-thread tokio runtime and serves `Server` over `rmcp::transport::stdio()`. `get_info` enables tools and sets `AGENT_RULES` as server instructions. `list_tools` returns one `Tool` from the core's definition (ENG-Engine: `TOOL_NAME`, `tool_description()`, `TOOL_INPUT_SCHEMA` parsed once at start; `session` optional string — omitted only for a keyless `enter`, HOST-3; `event` optional string; `params` object with string values; HOST-13). `call_tool` rejects other names with `invalid_params`, then runs `call` in `spawn_blocking` while holding the `calls` mutex. `call` validates argument types (non-string param → "param X must be a string"), picks `Runtime::for_session(key)` (or lookup from the server's cwd when keyless), then `Engine::call` (view when there is no event, else fire, with a `Bind { harness: "mcp" }` for keyless `enter`). The loaded config comes from the server's `ConfigCache` (`cache.rs`, HOST-16): keyed by the config files the call discovers (`Runtime::call_configs`); each call `stat`s every file the cached load depended on (`Loaded::inputs`: the configs, their machine files and the prompt files those name, found or not), looks again for one that was missing, and re-reads and compares a file whose modification time or size changed, or whose modification time is not older than the cache's build time minus a 2-second margin (git's racy-clean rule; the margin covers a file system clock that differs from the process's); any difference rebuilds, the rest are trusted, so after a tick a call reads no config file (PLAN-008 D8-8, DC-3). A rejected event or error returns `isError: true` with the text.

IMPLEMENTS: HOST-12_AC-1, HOST-13_AC-1, HOST-16_AC-1, CFG-16_AC-1, CLI-9_AC-1

```rust
// cache.rs: one entry per set of config files; each input as last seen (time, size, bytes) or missing
pub struct ConfigCache { /* entries: configs, inputs, built, runtime */ }
impl ConfigCache { pub fn runtime(&mut self, files: &[ConfigFile]) -> Result<&mut Runtime>; }
pub fn call(args: &Map<String, Value>, cwd: &Path, explicit: Option<&Path>) -> (bool, String);
pub fn serve(explicit: Option<&Path>) -> Result<u8>;

impl rmcp::ServerHandler for Server {
    fn get_info(&self) -> ServerConfig;
    async fn list_tools(&self, _: Option<PaginatedRequestParams>, _: RequestContext<RoleServer>)
        -> Result<ListToolsResult, ErrorData>;
    async fn call_tool(&self, request: CallToolRequestParams, _: RequestContext<RoleServer>)
        -> Result<CallToolResponse, ErrorData>;
}
```

### HOST-Wasm

`wasm_bindgen` `Engine` class over `smllm compile` JSON with a `MemoryStore`. The WASM store writes through: each saved session, binding and instance goes to the host's `put(kind, key, recordJson)` as it is saved, as history does (HOST-14, PLAN-008 D8-16), so a host persists what changed, never a snapshot per call (`exportState` measured 0.91 ms and 210 KB at 1,000 instances); a throwing `put` is reported like a throwing `history`. `tool()` returns the core's tool definition as JSON; `callTool(argsJson)` parses the agent's arguments with the MCP server's rules and error texts and runs `Engine::call`, returning the reply, a malformed call as `ok: false` with `error: …` (HOST-13). One JS host object supplies every host trait: `supports`, `check`, `run` (`""` = success), `read`, `isMatch`, `now`, `random`, and `history(machine, id, entryJson)`, which receives each history entry: the WASM store (`WasmStore`, over the `MemoryStore`) keeps state only and hands the log to the host, since the engine never reads history back; a throwing `history` loses that entry, never the transition (PLAN-006 D6-4); params and env cross as JSON strings, read and written with `smllm-json` (ENG-Json, PLAN-007 D7-6); a malformed constructor argument, `importState` or `fire` params is reported as `invalid compiled machines…`, `invalid state…` or `params must be a JSON object`, followed by where it failed (`: machines[0].id: expected a string, found an integer`, `: unexpected character at byte 4`), and a non-string param as `param <k> must be a string`. Every method returns the core `Reply` as JSON; `stop` returns `{"decision": "allow" | "block" | "runaway", "text"?}` as JSON (`text`, the events list, for block and runaway; PLAN-003 D3-4). Every host method is imported with `catch`: a throw becomes a failed guard or action, an `Err` from `read`/`isMatch` (so a pattern JS rejects is the core's graceful bad-pattern rejection), `false` from `supports`, `0` from `now`, and a counter from `random`; the text is `host <method> threw: <String(e)>` (PLAN-003 F22); `exportState` / `importState` move sessions, bindings and instances as JSON so a JS host can persist them; the host owns persistence and retention. `unsupported()` lists guard/action kinds the host cannot run (NFR-9). That string API is `smllm-wasm/raw`; the package's main export is a typed wrapper over it (PLAN-006 D6-3): `packages/smllm-wasm/wrap.js`, hand-written with `types.d.ts` (no TypeScript build), for the `web` (`index.js`, re-exporting `init` / `initSync`) and `bundler` (`bundler.js`) builds. Its `Engine` takes compiled machines as text or an object, returns objects (`Reply`, `StopDecision`, `SessionStatus`, state), and adapts an object-based host whose methods are all optional (`isMatch`, `now`, `random` default to `RegExp`, `Date.now`, `crypto.getRandomValues`; `history` to dropping).

```rust
#[wasm_bindgen]
impl Engine {
    #[wasm_bindgen(constructor)]
    pub fn new(compiled: &str, host: JsHost) -> Result<Engine, JsError>;
    pub fn unsupported(&mut self) -> String;
    pub fn bind(&mut self, harness: &str, host_session: Option<String>, cwd: &str) -> Result<String, JsError>;
    pub fn view(&mut self, key: &str) -> Result<String, JsError>;
    pub fn events(&mut self, key: &str) -> Result<String, JsError>;
    pub fn fire(&mut self, key: Option<String>, event: &str, params: &str) -> Result<String, JsError>;
    pub fn stop(&mut self, key: &str, stop_hook_active: bool) -> Result<String, JsError>;
    #[wasm_bindgen(js_name = promptSubmitted)]
    pub fn prompt_submitted(&mut self, key: &str) -> Result<(), JsError>;
    #[wasm_bindgen(js_name = exportState)]
    pub fn export_state(&self) -> String;
    #[wasm_bindgen(js_name = importState)]
    pub fn import_state(&mut self, state: &str) -> Result<(), JsError>;
    pub fn tool(&self) -> String;
    #[wasm_bindgen(js_name = callTool)]
    pub fn call_tool(&mut self, args: &str) -> String;
}
```

### HOST-JsPackage

`packages/smllm-wasm`, in TypeScript (`src/*.ts`, compiled by `tsc` in `build:wasm` to `.js` and generated `.d.ts`; PLAN-008 D8-18, replacing PLAN-006 D6-3's hand-kept declarations). The typed `Engine` adds `tool()` → `{ name, description, inputSchema }` and `callTool({ session?, event?, params? })` → `Reply`. `Storage` is `{ load(): State | undefined; put(kind, key, record): void; history(machine, id, entry): void }`; `load` runs once, into `importState`. Shipped: `memoryStorage()`, and in `smllm-wasm/node` `nodeFileStorage(dir)` (a file per record, written atomically via temp + rename; file names from keys as the file store names ref markers: lowercase-safe, a hash suffix when needed; history JSONL per instance in the CLI's line format; one process per folder, DC-4) and `nodeHost({ configDir, timeoutSecs? })`, a command host per DEC-4..DEC-7: string `run` through `sh -c` (`cmd /C` on Windows), a list without a shell, `params.cwd` against `configDir` (compiled paths are relative to the config, D8-22) else the session's `cwd`, `SMLLM_*` env, stdin closed, the last 400 characters of output on failure, and the CLI's failure texts. Host methods are synchronous, so it uses `spawnSync`; on unix a small POSIX `sh` supervisor (`set -m`) starts the command in its own process group, a background timer kills the group on expiry, and the timer is stopped as soon as the command is reaped (before its group id could be reused); on Windows `spawnSync`'s timeout kills the command's process, as the CLI (D8-15).

IMPLEMENTS: HOST-13_AC-2, HOST-14_AC-1, HOST-14_AC-2, HOST-15_AC-1

```ts
export class Engine { tool(): Tool; callTool(args: ToolArgs): Reply; /* … as before */ }
export interface Storage { load(): State | undefined; put(kind: "session" | "binding" | "instance", key: string, record: object): void;
                           history(machine: string, id: string, entry: HistoryEntry): void }
export function memoryStorage(): Storage;
// smllm-wasm/node
export function nodeFileStorage(dir: string): Storage;
export function nodeHost(options: { configDir: string; timeoutSecs?: number }): Host;
```

## Data Models

### Core Types

- ANSWER: the kit's Claude Code hook answer

```rust
pub enum Answer {
    Allow { stderr: Option<String> },         // {}
    Block { reason: String },                 // {"decision":"block","reason":…}
    Context { event: String, context: String }, // hookSpecificOutput.additionalContext
}
```

- HOOK_INPUT: every field optional, unknown fields ignored

```rust
pub struct HookInput {
    pub session_id: Option<String>, pub transcript_path: Option<String>, pub cwd: Option<String>,
    pub hook_event_name: Option<String>, pub source: Option<String>,
    pub prompt: Option<String>, pub stop_hook_active: Option<bool>,
}
```

### Entities

### Claude Code plugin
`plugin/` with `.claude-plugin/plugin.json`, `hooks/hooks.json` (the three `smllm harness hook claude <HOOK>` commands), `.mcp.json` (`smllm mcp`); listed by `.claude-plugin/marketplace.json` (`source: ./plugin`)
- VERSION (string, required): kept equal to the workspace version

## Correctness Properties

- HOST_P-1 [Hook output shape]: a hook either exits 0 with exactly one JSON object line on stdout, or exits 1 with empty stdout and `error:` on stderr
  VALIDATES: HOST-7_AC-1, NFR-4
- HOST_P-2 [Install idempotent]: a second `harness install` with the same options writes nothing and reports every part current
  VALIDATES: HOST-6_AC-1, HOST-11_AC-1
- HOST_P-3 [Region isolation]: installing changes only the text between smllm's markers
  VALIDATES: HOST-11_AC-1

Kit semantics behind these (PLAN-003 F19–F21): a declined part is never read, so a broken file only a declined part writes cannot block the rest; the tool owns its hook commands (`hooks[].command` starting with its prefix), not the hook group they sit in, so a user's command and keys like `matcher` in the same group are kept, not "edited", and `--force` rewrites only the tool's commands (a group left empty on removal goes); the region is the one a closing marker ends, opened by the nearest opening marker before it, so a stray opening marker stays the user's text.

## Error Handling

### Hook failures

- UNKNOWN HOOK: `error: no hook named …` on stderr, exit 1
- UNKNOWN HARNESS: `error: no harness named …`, exit 1 (a failed hook, HOST-7)
- STORE OR ENGINE ERROR: exit 1, stderr only

### MCP call failures

- BAD ARGUMENT TYPE: tool result `isError`, text `error: … must be a string`
- UNKNOWN TOOL: JSON-RPC `invalid_params` error
- REJECTED EVENT: tool result `isError` with the error block and events list

### Strategy

PRINCIPLES:

- A hook never blocks on its own failure; the agent can always stop
- `harness install` refuses before any write when a part is edited (without `--force`) or the declined store is unreadable
- Tool errors are returned as tool results, so the agent sees them

## Testing Strategy

### Property-Based Testing

- FRAMEWORK: proptest
- MINIMUM_ITERATIONS: 64
- TAG_FORMAT: @zen-test: HOST_P-{n}

The kit's `tests/properties_harness.rs` covers HOST_P-2 and HOST_P-3 (idempotent install, region isolation, merges keep the rest, refusal writes nothing) without `@zen-test` markers.

### Unit Testing

- AREAS: kit `Answer` JSON forms, region/merge/target selection, declined store

### Integration Testing

`crates/app/smllm/tests/session.rs` drives the real binary as a fake agent; `tests/cli.rs` covers install/status.

- SCENARIOS: hooks + `fire` through the showcase (idle stop, block until yield, runaway, prompt clears yield, takeover, moved), no-config silent hooks, keyless `fire enter`, MCP over stdio (initialize, tools/list, calls, type errors, unknown key, unknown tool), `harness install|status` project scope incl. `AGENTS.md` target and `--without mcp`

## Requirements Traceability

SOURCE: .zen/specs/REQ-HOST-harnesses.md

- HOST-1_AC-1 → ENG-Engine — core `bind`/`view`/`menu`/`fire`; used by HOST-Runtime and HOST-Wasm; no marker
- HOST-2_AC-1 → ENG-Engine — `sm-` + 6 random chars, retried until unused; no marker
- HOST-3_AC-1 → ENG-Engine — keyless `enter` via HOST-Mcp and `smllm fire`; tested in session.rs without HOST marker
- HOST-4_AC-1 → ENG-Engine — binding reuse in `bind`; new id → new key tested in session.rs without marker
- HOST-5_AC-1 → HOST-Mcp — and CLI-Commands `fire`; no marker
- HOST-6_AC-1 → HOST-Claude (HOST_P-2)
- HOST-6_AC-2 → HOST-Claude — plugin files under `plugin/`; not tested
- HOST-6_AC-3 → HOST-Claude — `ClaudeMcpUser`; not tested (needs the `claude` CLI)
- HOST-7_AC-1 → HOST-Claude (HOST_P-1)
- HOST-8_AC-1 → HOST-Claude
- HOST-9_AC-1 → HOST-Claude — spike run 2026-09-25: injection, same `session_id` on resume, MCP call, Stop block → `yield`, `stop_hook_active` all confirmed (PLAN-001 §15)
- HOST-10_AC-1 → HOST-Claude — logic in agent-harness-kit `harness::target`; kit tests and cli.rs cover it without marker
- HOST-11_AC-1 → HOST-Claude (HOST_P-3) — logic in agent-harness-kit `harness::region`; kit tests without marker
- HOST-12_AC-1 → HOST-Mcp [partial] content asserted over stdio; no snapshot of the description yet
- HOST-13_AC-1 → ENG-Engine (tool definition), HOST-Mcp, HOST-Wasm (`tool()`)
- HOST-13_AC-2 → ENG-Engine (`call`), HOST-Wasm (`callTool`), HOST-JsPackage — one call table answered alike by MCP and wasm
- HOST-14_AC-1 → HOST-Wasm (write-through `put`, `history`)
- HOST-14_AC-2 → HOST-JsPackage (`memoryStorage`, `nodeFileStorage`) — a restored engine equals the original
- HOST-15_AC-1 → HOST-JsPackage (`nodeHost`) — Node tests mirroring the CLI runner's, a grandchild killed on timeout
- HOST-16_AC-1 → HOST-Mcp (`ConfigCache`) — an edit within one timestamp tick picked up

## Library Usage

### Framework Features

- RMCP SERVERHANDLER: `get_info`, `list_tools`, `call_tool`; `transport::stdio()`; `ServiceExt::serve`
- TOKIO: current-thread runtime, `spawn_blocking`
- WASM-BINDGEN: `extern "C"` JS host type with methods; exported class with constructor

### External Libraries

- rmcp (3): MCP server
- tokio (1): runtime for `smllm mcp`
- agent-harness-kit (0.1): parts, install/status, hook answers
- wasm-bindgen (0.2): JS bindings
- smllm-json (0.1): smllm-wasm's JSON

## Change Log

- 0.1.0 (2026-09-25): Initial design
- 0.2.0 (2026-09-28): smllm-wasm JSON via smllm-json (PLAN-007)
- 0.3.0 (2026-09-28): The core's tool definition and `Engine::call` in MCP and wasm; the MCP config cache; write-through wasm storage; HOST-JsPackage (TypeScript, Storage, nodeHost) (PLAN-008)
