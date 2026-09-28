// The scaling check (NFR-10_AC-4): an event's time with 1,000 paused
// instances must stay within 5x its time with 100. Absolute times on shared
// CI runners are noise, but a path that grows with the store (a scan, a
// quadratic sort) shows up as a ratio. `listPaused` is not measured: its
// output grows with the list by design.
// @zen-test: NFR-10_AC-4
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const pkg = join(here, "..");
const { initSync, Engine } = await import(join(pkg, "index.js"));
initSync({ module: readFileSync(join(pkg, "wasm/web/smllm_wasm_bg.wasm")) });
const compiled = JSON.parse(readFileSync(join(here, "dev.json"), "utf8"));

// Below this, a call is at the timer's resolution: the ratio says nothing.
const FLOOR_MS = 0.05;
const RATIO = 5;
const REPS = 40;

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const ms = (f) => {
  const t = process.hrtime.bigint();
  f();
  return Number(process.hrtime.bigint() - t) / 1e6;
};

test("event time stays flat from 100 to 1,000 paused instances", () => {
  const e = new Engine(compiled);
  const key = e.bind("scaling", "s", "/work").session;
  let n = 0;
  const pauseOne = () => {
    e.fire(key, "enter", { stateMachine: "dev", issueId: `GH-${++n}` });
    e.fire(key, "pause");
  };
  const measure = () => {
    const t = { enter: [], view: [], stop: [], pause: [] };
    for (let i = 0; i < REPS; i++) {
      t.enter.push(ms(() => e.fire(key, "enter", { stateMachine: "dev", issueId: `GH-${++n}` })));
      t.view.push(ms(() => e.view(key)));
      t.stop.push(ms(() => e.stop(key, false)));
      t.pause.push(ms(() => e.fire(key, "pause")));
    }
    return Object.fromEntries(Object.entries(t).map(([k, v]) => [k, median(v)]));
  };
  while (n < 100) pauseOne();
  measure(); // warm up
  const small = measure();
  while (n < 1000) pauseOne();
  const large = measure();
  for (const call of Object.keys(small)) {
    const limit = RATIO * Math.max(small[call], FLOOR_MS / RATIO);
    assert.ok(
      large[call] <= Math.max(limit, FLOOR_MS),
      `${call}: ${large[call].toFixed(3)} ms with 1,000 paused vs ${small[call].toFixed(3)} ms with 100`,
    );
  }
  e.free();
});
