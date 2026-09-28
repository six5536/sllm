# smllm-wasm

smllm's engine (`smllm-core`) as WebAssembly, for hosts that run state
machines in a browser or Node while the LLM runs elsewhere. Typed: objects
in, objects out (TypeScript declarations included).

```js
import init, { Engine } from "smllm-wasm"; // or { Engine } from "smllm-wasm/bundler"

await init(); // the bundler build needs no init
const compiled = await (await fetch("machines.json")).json(); // `smllm compile`
const engine = new Engine(compiled, {
  // Every host method is optional; these are the ones worth knowing:
  history: (machine, id, entry) => save(entry), // each history entry, as it happens
  // supports / check / run: host guard and action kinds (`command`); none by default
});
const { session, text } = engine.bind("web");
// Show `text` to the agent; when it picks an event:
const reply = engine.fire(session, "enter", { stateMachine: "dev" });
// When the agent tries to stop (`stopHookActive`: it is already continuing
// because of an earlier block):
const stop = engine.stop(session, stopHookActive);
// stop.decision "block": hold the agent, show it stop.text (the events list);
// "runaway": let it stop, show stop.text to the user; "allow": let it stop.
```

Host methods and their defaults: `supports(kind)` (none), `check(kind, params,
env, cwd)`, `run(kind, params, env, cwd)` (return nothing on success, else the
failure detail), `read(file)` (`smllm compile` inlines prompt files),
`isMatch(pattern, value)` (`new RegExp`), `now()` (`Date.now`), `random()`
(`crypto.getRandomValues`), `history(machine, id, entry)` (dropped). A host
method that throws is a host failure: a failed guard or action, or a rejected
pattern, shown in the reply text. The engine stays usable.

Sessions and instances live in memory; persist them with `exportState()` and
restore them with `importState(state)`. History is not kept: each entry goes
to `history` as it happens, for you to store or drop, so the state holds
current state only. `unsupported()` lists guard and action kinds the machines
use that your host does not support. `free()` releases the wasm memory.

The string API underneath (JSON text in and out) is `smllm-wasm/raw` and
`smllm-wasm/raw/bundler`.

MIT licensed. See <https://github.com/six5536/smllm>.
