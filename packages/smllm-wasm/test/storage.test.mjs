// Write-through storage (HOST-14): each saved record reaches the storage as
// it is saved, and an engine restored from a storage equals the original.
// @zen-test: HOST-14_AC-1
// @zen-test: HOST-14_AC-2
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const pkg = join(here, "..");
const { initSync, Engine, memoryStorage } = await import(join(pkg, "dist/index.js"));
const { nodeFileStorage, fileName } = await import(join(pkg, "dist/node/index.js"));
initSync({ module: readFileSync(join(pkg, "wasm/web/smllm_wasm_bg.wasm")) });
const compiled = JSON.parse(readFileSync(join(here, "dev.json"), "utf8"));

/** A session with a paused, an active and an interrupted instance. */
function work(engine) {
  const key = engine.bind("test", "t-1", "/work").session;
  engine.fire(key, "enter", { stateMachine: "dev", issueId: "GH-1" });
  engine.fire(key, "pause");
  engine.fire(key, "enter", { stateMachine: "dev", issueId: "GH-2" });
  engine.fire(key, "unmatched");
  return key;
}

for (const [name, make] of [
  ["memory", () => [memoryStorage(), () => {}]],
  [
    "node files",
    () => {
      const dir = mkdtempSync(join(tmpdir(), "smllm-storage-"));
      return [nodeFileStorage(dir), () => rmSync(dir, { recursive: true, force: true }), dir];
    },
  ],
]) {
  test(`an engine restored from ${name} storage equals the original`, () => {
    const [storage, done, dir] = make();
    try {
      const engine = new Engine(compiled, {}, storage);
      const key = work(engine);
      const again = new Engine(compiled, {}, storage);
      assert.deepEqual(again.exportState(), engine.exportState());
      assert.equal(again.status(key).interrupted.label, "GH-2");
      if (dir) {
        assert.deepEqual(readdirSync(dir).sort(), ["bindings", "history", "instances", "sessions"]);
        const [file] = readdirSync(join(dir, "history", "dev"));
        const log = readFileSync(join(dir, "history", "dev", file), "utf8");
        assert.ok(log.split("\n").filter(Boolean).every((l) => JSON.parse(l).event), log);
      } else {
        assert.equal([...storage.log.values()].flat().length > 0, true);
      }
    } finally {
      done();
    }
  });
}

test("a call writes what changed, not the whole state", () => {
  const puts = [];
  const engine = new Engine(compiled, { put: (kind, key) => puts.push(`${kind} ${key}`) });
  const key = engine.bind("test", "t-2", "/work").session;
  const cycle = (n) => {
    engine.fire(key, "enter", { stateMachine: "dev", issueId: `GH-${n}` });
    engine.fire(key, "pause");
  };
  const writes = [];
  for (let n = 1; n <= 1000; n++) {
    puts.length = 0;
    cycle(n);
    if (n === 100 || n === 1000) writes.push(puts.length);
  }
  assert.equal(writes[0], writes[1], "writes per cycle grew with the store");
  assert.ok(writes[0] <= 6, `${writes[0]} writes for one enter and pause`);
});

test("a throwing put loses the record, never the transition", () => {
  const engine = new Engine(compiled, {
    put: () => {
      throw new Error("disk full");
    },
  });
  const key = engine.bind("test", "t-3", "/work").session;
  const r = engine.fire(key, "enter", { stateMachine: "dev", issueId: "GH-7" });
  assert.equal(r.ok, true, r.text);
  assert.equal(engine.status(key).state, "TRIAGE");
});

test("file names never merge two keys", () => {
  assert.equal(fileName("sm-ab12"), "sm-ab12");
  assert.notEqual(fileName("Dev/i-1"), fileName("dev/i-1"));
  assert.equal(fileName("claude/Ab"), "claude%2f%41b");
  const long = fileName("x".repeat(300));
  assert.ok(long.length <= 120, long);
  assert.notEqual(long, fileName(`${"x".repeat(299)}y`));
});
