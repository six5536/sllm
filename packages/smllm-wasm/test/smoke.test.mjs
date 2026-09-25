// Smoke test: load the web build in Node, drive one scripted session (TEST-3).
// @zen-test: TEST-3_AC-1
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const pkg = join(here, "..");
const { initSync, Engine } = await import(join(pkg, "wasm/web/smllm_wasm.js"));
initSync({ module: readFileSync(join(pkg, "wasm/web/smllm_wasm_bg.wasm")) });

const compiled = readFileSync(join(here, "dev.json"), "utf8");
let n = 0;
const host = {
  supports: (kind) => kind === "command",
  check: () => true,
  run: () => "",
  read: () => undefined,
  isMatch: (pattern, value) => new RegExp(pattern).test(value),
  now: () => 1_790_332_320_000,
  random: () => ++n,
};

test("a scripted session runs through the JS API", () => {
  const engine = new Engine(compiled, host);
  assert.equal(engine.unsupported(), "[]");
  const idle = JSON.parse(engine.bind("test", "t-1", "/work"));
  assert.ok(idle.text.startsWith("<smllm>\nsession sm-"), idle.text);
  const key = idle.session;
  let r = JSON.parse(engine.fire(key, "enter", JSON.stringify({ stateMachine: "dev", issueId: "GH-1" })));
  assert.equal(r.ok, true, r.text);
  assert.equal(r.location.state, "TRIAGE");
  assert.ok(engine.stop(key, false).includes("<events>"));
  r = JSON.parse(engine.fire(key, "accept", "{}"));
  assert.equal(r.location.state, "WORK", r.text);
  assert.throws(() => engine.fire(key, "submit", JSON.stringify({ summary: 1 })), /must be a string/);
  r = JSON.parse(engine.fire(key, "yield", "{}"));
  assert.equal(engine.stop(key, false), undefined);
  const status = JSON.parse(engine.status(key));
  assert.equal(status.state, "WORK");
  assert.equal(status.yielded, true);
  assert.equal(status.instance.label, "GH-1");
  const state = engine.exportState();
  const again = new Engine(compiled, host);
  again.importState(state);
  assert.equal(JSON.parse(again.view(key)).location.state, "WORK");
});
