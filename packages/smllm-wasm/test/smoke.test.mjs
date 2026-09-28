// Smoke test: load the web build in Node, drive one scripted session (TEST-3).
// @zen-test: TEST-3_AC-1
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const pkg = join(here, "..");
// The package's main entry: the typed wrapper (PLAN-006 D6-3).
const { initSync, Engine } = await import(join(pkg, "index.js"));
const raw = await import(join(pkg, "wasm/web/smllm_wasm.js"));
initSync({ module: readFileSync(join(pkg, "wasm/web/smllm_wasm_bg.wasm")) });

const compiled = JSON.parse(readFileSync(join(here, "dev.json"), "utf8"));
let n = 0;
const host = {
  supports: (kind) => kind === "command",
  check: () => true,
  run: () => "",
  read: () => undefined,
  isMatch: (pattern, value) => new RegExp(pattern).test(value),
  now: () => 1_790_332_320_000,
  random: () => ++n,
  history: (machine, id, entry) => log.push({ machine, id, entry }),
};
const log = [];

test("a scripted session runs through the JS API", () => {
  const engine = new Engine(compiled, host);
  assert.deepEqual(engine.unsupported(), []);
  const idle = engine.bind("test", "t-1", "/work");
  assert.ok(idle.text.startsWith("<smllm>\nsession sm-"), idle.text);
  const key = idle.session;
  let r = engine.fire(key, "enter", { stateMachine: "dev", issueId: "GH-1" });
  assert.equal(r.ok, true, r.text);
  assert.equal(r.location.state, "TRIAGE");
  const block = engine.stop(key, false);
  assert.equal(block.decision, "block");
  assert.ok(block.text.includes("<events>"));
  assert.equal(engine.stop(key, true).decision, "runaway");
  r = engine.fire(key, "accept");
  assert.equal(r.location.state, "WORK", r.text);
  assert.throws(() => engine.fire(key, "submit", { summary: 1 }), /must be a string/);
  r = engine.fire(key, "yield");
  assert.deepEqual(engine.stop(key), { decision: "allow" });
  const status = engine.status(key);
  assert.equal(status.state, "WORK");
  assert.equal(status.yielded, true);
  assert.equal(status.instance.label, "GH-1");
  const state = engine.exportState();
  // History goes to the host, not into the snapshot (PLAN-006 D6-4).
  assert.ok(!("history" in state), JSON.stringify(state));
  assert.equal(log[0].machine, "dev");
  assert.deepEqual(log.map((h) => h.entry.event), ["enter", "accept", "yield"]);
  const again = new Engine(compiled, host);
  again.importState(state);
  assert.equal(again.view(key).location.state, "WORK");
  again.free();
});

// The string API underneath stays usable as `smllm-wasm/raw`.
test("the raw API takes and returns JSON text", () => {
  const engine = new raw.Engine(JSON.stringify(compiled), { ...host, history: () => {} });
  const idle = JSON.parse(engine.bind("test", "t-9", "/work"));
  assert.equal(JSON.parse(engine.view(idle.session)).session, idle.session);
});

// With no host methods, the defaults run the machine (no command kinds).
test("an empty host has working defaults", () => {
  const engine = new Engine(compiled);
  const key = engine.bind("test", undefined).session;
  assert.equal(engine.fire(key, "enter", { stateMachine: "dev", issueId: "GH-3" }).location.state, "TRIAGE");
  assert.ok(engine.unsupported().length > 0);
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
  const key = engine.bind("test", "t-2", "/work").session;
  for (let i = 0; i < 600; i++) {
    const r = engine.fire(key, "enter", { stateMachine: "dev", issueId: "GH-1" });
    assert.equal(r.ok, false);
    assert.match(r.text, /host isMatch threw: SyntaxError/);
  }
  assert.equal(engine.view(key).session, key);
  assert.ok(JSON.stringify(engine.exportState()).includes(key));
});

// A throwing `history` loses the entry, never the transition (PLAN-006 D6-4).
test("a throwing history callback keeps the transition", () => {
  const engine = new Engine(compiled, {
    ...host,
    history: () => {
      throw new Error("log is down");
    },
  });
  const key = engine.bind("test", "t-3", "/work").session;
  const r = engine.fire(key, "enter", { stateMachine: "dev", issueId: "GH-2" });
  assert.equal(r.ok, true, r.text);
  assert.equal(engine.status(key).state, "TRIAGE");
});

// miniserde errors carry no detail; the engine says what could not be read
// (PLAN-007 D7-1), and bad input never breaks it.
test("malformed JSON is rejected with what failed", () => {
  assert.throws(() => new Engine("{", host), /invalid compiled machines/);
  const engine = new Engine(compiled, host);
  assert.throws(() => engine.importState("{"), /invalid state/);
  const key = engine.bind("test", "t-4", "/work").session;
  assert.throws(() => engine.fire(key, "enter", "[1]"), /params must be a JSON object/);
  assert.equal(engine.fire(key, "enter", { stateMachine: "dev", issueId: "GH-4" }).ok, true);
});
