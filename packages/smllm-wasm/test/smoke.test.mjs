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
  history: (machine, id, entry) => log.push({ machine, id, entry: JSON.parse(entry) }),
};
const log = [];

test("a scripted session runs through the JS API", () => {
  const engine = new Engine(compiled, host);
  assert.equal(engine.unsupported(), "[]");
  const idle = JSON.parse(engine.bind("test", "t-1", "/work"));
  assert.ok(idle.text.startsWith("<smllm>\nsession sm-"), idle.text);
  const key = idle.session;
  let r = JSON.parse(engine.fire(key, "enter", JSON.stringify({ stateMachine: "dev", issueId: "GH-1" })));
  assert.equal(r.ok, true, r.text);
  assert.equal(r.location.state, "TRIAGE");
  const block = JSON.parse(engine.stop(key, false));
  assert.equal(block.decision, "block");
  assert.ok(block.text.includes("<events>"));
  assert.equal(JSON.parse(engine.stop(key, true)).decision, "runaway");
  r = JSON.parse(engine.fire(key, "accept", "{}"));
  assert.equal(r.location.state, "WORK", r.text);
  assert.throws(() => engine.fire(key, "submit", JSON.stringify({ summary: 1 })), /must be a string/);
  r = JSON.parse(engine.fire(key, "yield", "{}"));
  assert.deepEqual(JSON.parse(engine.stop(key, false)), { decision: "allow" });
  const status = JSON.parse(engine.status(key));
  assert.equal(status.state, "WORK");
  assert.equal(status.yielded, true);
  assert.equal(status.instance.label, "GH-1");
  const state = engine.exportState();
  // History goes to the host, not into the snapshot (PLAN-006 D6-4).
  assert.ok(!("history" in JSON.parse(state)), state);
  assert.deepEqual(log.map((h) => h.entry.event), ["enter", "accept", "yield"]);
  const again = new Engine(compiled, host);
  again.importState(state);
  assert.equal(JSON.parse(again.view(key)).location.state, "WORK");
});

// A host method that throws is a host failure, not a broken engine
// (PLAN-003 F22): here `new RegExp` rejects a Rust-only pattern.
test("a throwing host leaves the engine usable", () => {
  const throwing = {
    ...host,
    isMatch: (pattern) => {
      throw new SyntaxError(`Invalid regular expression: /${pattern}/`);
    },
  };
  const engine = new Engine(compiled, throwing);
  const key = JSON.parse(engine.bind("test", "t-2", "/work")).session;
  for (let i = 0; i < 600; i++) {
    const r = JSON.parse(engine.fire(key, "enter", JSON.stringify({ stateMachine: "dev", issueId: "GH-1" })));
    assert.equal(r.ok, false);
    assert.match(r.text, /host isMatch threw: SyntaxError/);
  }
  assert.equal(JSON.parse(engine.view(key)).session, key);
  assert.ok(engine.exportState().includes(key));
});

// A throwing `history` loses the entry, never the transition (PLAN-006 D6-4).
test("a throwing history callback keeps the transition", () => {
  const engine = new Engine(compiled, {
    ...host,
    history: () => {
      throw new Error("log is down");
    },
  });
  const key = JSON.parse(engine.bind("test", "t-3", "/work")).session;
  const r = JSON.parse(engine.fire(key, "enter", JSON.stringify({ stateMachine: "dev", issueId: "GH-2" })));
  assert.equal(r.ok, true, r.text);
  assert.equal(JSON.parse(engine.status(key)).state, "TRIAGE");
});
