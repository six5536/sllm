// The agent loop in Node, with the Node host: real `command` guards and
// actions (nodeHost) and file storage (nodeFileStorage). It runs twice on one
// storage folder: the second run is a new engine that continues where the
// first stopped. Run it with `node examples/wasm/node.ts` after
// `npm run build:wasm`; CI runs it (TEST-3_AC-2).

import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { Engine, initSync } from "smllm-wasm";
import { nodeFileStorage, nodeHost } from "smllm-wasm/node";

import { runAgent, type Step } from "./agent-loop.ts";

const here = dirname(fileURLToPath(import.meta.url));
// The web build needs its wasm once; `smllm-wasm/raw` sits beside it.
initSync({ module: readFileSync(new URL(import.meta.resolve("smllm-wasm/raw").replace(/\.js$/, "_bg.wasm"))) });
const compiled = readFileSync(join(here, "machines.json"), "utf8");

const work = mkdtempSync(join(tmpdir(), "smllm-example-"));
const files = nodeFileStorage(join(work, "state"));
// Commands run in the session's working directory: `work`. A command's
// `cwd` param would be relative to this folder, the compiled config's.
const commands = nodeHost({ configDir: here });
// Time in commands (real processes) and storage (file writes), kept apart
// from smllm's own.
const ms = { commands: 0, storage: 0 };
const timed = <A extends unknown[], R>(into: keyof typeof ms, f: (...a: A) => R) => (...a: A): R => {
  const t = performance.now();
  try {
    return f(...a);
  } finally {
    ms[into] += performance.now() - t;
  }
};
const host = { ...commands, check: timed("commands", commands.check!), run: timed("commands", commands.run!) };
const storage = {
  load: files.load,
  put: timed("storage", files.put),
  history: timed("storage", files.history),
};

try {
  // Run 1: start a note, submit too little (the guard fails), try to stop
  // (blocked: the events list), pause it, stop.
  const first: Step[] = [
    { call: { session: "{session}", event: "enter", params: { stateMachine: "note", noteId: "N-1" } } },
    { call: { session: "{session}", event: "submit", params: { text: "too short" } } },
    { stop: true },
    { call: { session: "{session}", event: "pause" } },
    { stop: true },
  ];
  const one = runAgent(new Engine(compiled, host, storage), first, "run-1", work);
  const afterOne = { ...ms };
  assert.match(one.texts[3]!, /Guard: command .* → false/, "the note was too short");
  assert.match(one.texts[4]!, /<events>/, "the stop was blocked with the events list");
  assert.equal(one.texts.at(-1), "(stopped)");

  // Run 2: a new engine from the same folder continues the paused note,
  // files it, then sends a malformed call (answered, never thrown).
  const second: Step[] = [
    { call: { session: "{session}", event: "enter", params: { stateMachine: "note", noteId: "N-1" } } },
    { call: { session: "{session}", event: "submit", params: { text: "Measured smllm in-process." } } },
    { call: { session: "{session}", event: 5 as unknown as string } },
    { stop: true },
  ];
  const two = runAgent(new Engine(compiled, host, storage), second, "run-2", work);
  assert.match(two.texts[2]!, /DRAFT \(visit 3\)[^]*Arrived by: enter\n/, "the second run continued the note");
  assert.match(two.texts[3]!, /Completed note N-1/);
  assert.equal(two.texts[4], "error: event must be a string");
  assert.equal(readFileSync(join(work, "notes.txt"), "utf8"), "Measured smllm in-process.\n");

  console.log(two.texts[3]);
  // Run 1's first calls also compile the wasm; run 2 (a new engine on the
  // compiled module) shows the steady cost a long-running host sees.
  const report = (name: string, run: typeof one, commands: number, stored: number) => {
    const per = (x: number) => `${(x / run.turns).toFixed(3)} ms`;
    const own = run.smllmMs - commands - stored;
    console.log(`${name}: ${run.turns} turns; per turn smllm ${per(own)}, storage ${per(stored)}, commands ${per(commands)}`);
  };
  report("run 1 (cold)", one, afterOne.commands, afterOne.storage);
  report("run 2 (warm)", two, ms.commands - afterOne.commands, ms.storage - afterOne.storage);
} finally {
  rmSync(work, { recursive: true, force: true });
}
