# smllm-wasm

smllm's engine (`smllm-core`) as WebAssembly, for hosts that run state
machines in-process: in a browser, a worker, a serverless function or Node,
while the LLM runs wherever it runs. Typed: objects in, objects out
(TypeScript declarations included).

```js
import init, { Engine, memoryStorage } from "smllm-wasm"; // or from "smllm-wasm/bundler"

await init(); // the bundler build needs no init
const compiled = await (await fetch("machines.json")).json(); // `smllm compile`
const engine = new Engine(
  compiled,
  {
    // Every host method is optional; supports / check / run add host guard and
    // action kinds (`command`), none by default.
  },
  memoryStorage(), // or your own Storage (below); optional
);
const { session, text } = engine.bind("web");
// Give the model the same `smllm` tool the MCP server offers, and pass its
// calls through: a malformed call comes back as an answer, never a throw.
const tool = engine.tool(); // { name, description, inputSchema }
const reply = engine.callTool({ session, event: "enter", params: { stateMachine: "dev" } });
// When the model tries to stop (`stopHookActive`: it is already continuing
// because of an earlier block):
const stop = engine.stop(session, stopHookActive);
// stop.decision "block": hold the model, show it stop.text (the events list);
// "runaway": let it stop, show stop.text to the user; "allow": let it stop.
```

`examples/wasm` in the repository is a whole agent loop, in TypeScript.

## Host

Host methods and their defaults: `supports(kind)` (none), `check(kind, params,
env, cwd)`, `run(kind, params, env, cwd)` (return nothing on success, else the
failure detail; a command's `params.cwd` is relative to the compiled config
file), `read(file)` (`smllm compile` inlines prompt files), `isMatch(pattern,
value)` (`new RegExp`), `now()` (`Date.now`), `random()`
(`crypto.getRandomValues`), `history(machine, id, entry)` and `put(kind, key,
record)` (dropped). A host method that throws is a host failure: a failed
guard or action, or a rejected pattern, shown in the reply text. The engine
stays usable.

## Storage

The engine works in memory and writes through: each session, binding and
instance goes to `put` as it is saved, and each history entry to `history`,
so persisting costs what changed, never a snapshot per call. A `Storage` is
`{ load(), put(kind, key, record), history(machine, id, entry) }`; the
constructor's third argument loads it once and then hands it every write.
`memoryStorage()` keeps it in memory; in a browser, write one over IndexedDB,
or send the records to a server. `exportState()` / `importState(state)` move
the whole state at once, for backups and moves.

## Node: `smllm-wasm/node`

```js
import { nodeFileStorage, nodeHost } from "smllm-wasm/node";

const engine = new Engine(
  compiled,
  nodeHost({ configDir: "/path/to/the/compiled/config/folder" }),
  nodeFileStorage("/path/to/state"),
);
```

`nodeHost` runs `command` guards and actions as the smllm CLI does: a string
`run` through the shell, a list without one, the `SMLLM_*` environment, a
timeout (60 s by default) that kills what the command started too, and the
last 400 characters of output on failure. `nodeFileStorage` keeps a file per
record, written atomically, and history as JSONL. One process owns a storage
folder.

## WASM or CLI?

Median time per call, measured by `npm run bench` in the repository (a Linux
container, local disk; store size = paused / completed instances):

| Path | Call | 0 / 0 | 100 / 1000 | 1000 / 1000 |
| ---- | ---- | ---: | ---: | ---: |
| wasm, in-process | `fire` enter / pause | 0.06 ms | 0.02 / 0.03 ms | 0.02 / 0.03 ms |
| wasm, in-process | view, stop | 0.02 ms | 0.006 ms | 0.006 ms |
| CLI, `smllm mcp` (one process, over stdio) | tool call | 0.2–0.4 ms | 0.1–0.6 ms | 0.1–2.3 ms |
| CLI, a process per call | `smllm fire`, `statusline` | 1.4–1.7 ms | 1.2–1.8 ms | 1.3–4.4 ms |

Use the wasm for browsers, workers and serverless functions (no binary, no
shell), for a model that answers in milliseconds, for guards and actions
written in JavaScript, and for your own storage. Use the CLI (the `smllm` npm
package; `smllm mcp` for tool calls) when a Node app wants the CLI's
behaviour as it is: files shared between processes with locking, the status
line, `smllm instance list`, and the Claude Code hooks.

The string API underneath (JSON text in and out) is `smllm-wasm/raw` and
`smllm-wasm/raw/bundler`.

MIT licensed. See <https://github.com/six5536/smllm>.
