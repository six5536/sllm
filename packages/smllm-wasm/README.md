# smllm-wasm

smllm's engine (`smllm-core`) as WebAssembly, for hosts that run state
machines in a browser or Node while the LLM runs elsewhere.

```js
import init, { Engine } from "smllm-wasm"; // or "smllm-wasm/bundler"

await init();
const compiled = await (await fetch("machines.json")).text(); // `smllm compile`
const engine = new Engine(compiled, {
  supports: (kind) => false,          // no `command` guards/actions in a browser
  check: (kind, params, env, cwd) => false,
  run: (kind, params, env, cwd) => "unsupported",
  read: (file) => undefined,          // `smllm compile` inlines prompt files
  isMatch: (pattern, value) => new RegExp(pattern).test(value),
  now: () => Date.now(),
  random: () => crypto.getRandomValues(new Uint32Array(1))[0],
  history: (machine, id, entry) => {}, // each history entry, as JSON: keep it or drop it
});
const { session, text } = JSON.parse(engine.bind("web", undefined, "/"));
// Show `text` to the agent; when it picks an event:
const reply = JSON.parse(engine.fire(session, "enter", JSON.stringify({ stateMachine: "dev" })));
// When the agent tries to stop (`stopHookActive`: it is already continuing
// because of an earlier block):
const stop = JSON.parse(engine.stop(session, stopHookActive));
// stop.decision "block": hold the agent, show it stop.text (the events list);
// "runaway": let it stop, show stop.text to the user; "allow": let it stop.
```

A host method that throws is treated as a host failure: a failed guard or
action, or a rejected pattern (`isMatch`), shown in the reply text. The
engine stays usable.

Sessions and instances live in memory; persist them with `exportState()` /
`importState(json)`. History is not kept: each entry goes to `history` as it
happens, for you to store or drop, so the snapshot holds current state only. `unsupported()` lists guard/action kinds the machines
use that your host does not support.

MIT licensed. See <https://github.com/six5536/smllm>.
