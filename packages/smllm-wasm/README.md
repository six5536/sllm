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
});
const { session, text } = JSON.parse(engine.bind("web", undefined, "/"));
// Show `text` to the agent; when it picks an event:
const reply = JSON.parse(engine.fire(session, "enter", JSON.stringify({ stateMachine: "dev" })));
```

Sessions and instances live in memory; persist them with `exportState()` /
`importState(json)`. `unsupported()` lists guard/action kinds the machines
use that your host does not support.

MIT licensed. See <https://github.com/six5536/smllm>.
