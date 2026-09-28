#!/usr/bin/env node
// Per-call time through the wasm engine (in-process), the MCP server and the
// CLI, at three store sizes (NFR-10). Prints a table; not a CI gate (shared
// runners are noisy: CI runs the scaling check instead, NFR-10_AC-4).
//
// Runs on a temporary copy of scripts/bench/ on local disk, never the repo's
// folder, with its own XDG state and config dirs, so it touches no real
// sessions. Needs `npm run build:wasm` first (for the wasm package); builds
// the release CLI itself.

import { execFileSync, spawn } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const REPS = Number(process.env.BENCH_REPS ?? 20);
// Store sizes: [paused, completed] (D8-1: MCP's target is at 100 / 1,000).
const SIZES = [
  [0, 0],
  [100, 1000],
  [1000, 1000],
];

execFileSync("cargo", ["build", "--release", "--locked", "-q", "-p", "smllm"], { cwd: root, stdio: "inherit" });
const smllm = join(root, "target/release/smllm");
const dir = mkdtempSync(join(tmpdir(), "smllm-bench-"));
cpSync(join(root, "scripts/bench"), dir, { recursive: true });
const env = { ...process.env, XDG_STATE_HOME: join(dir, "xdg-state"), XDG_CONFIG_HOME: join(dir, "xdg-config") };
const config = join(dir, "config.toml");

const now = () => Number(process.hrtime.bigint()) / 1e6;
const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const rows = [];
const record = (path, size, op, times) => rows.push({ path, size: `${size[0]} / ${size[1]}`, op, ms: median(times) });

// --- wasm, in-process ------------------------------------------------------
async function wasm() {
  const pkg = join(root, "packages/smllm-wasm");
  const { initSync, Engine } = await import(join(pkg, "index.js"));
  initSync({ module: readFileSync(join(pkg, "wasm/web/smllm_wasm_bg.wasm")) });
  const compiled = execFileSync(smllm, ["compile", config], { env, encoding: "utf8" });
  const e = new Engine(compiled);
  const key = e.bind("bench", "b", dir).session;
  let n = 0;
  const seed = ([paused, completed], have) => {
    for (; have[0] < paused; have[0]++) {
      e.fire(key, "enter", { stateMachine: "bench", taskId: `P-${n++}` });
      e.fire(key, "pause");
    }
    for (; have[1] < completed; have[1]++) {
      e.fire(key, "enter", { stateMachine: "bench", taskId: `C-${n++}` });
      e.fire(key, "finish");
    }
  };
  const have = [0, 0];
  for (const size of SIZES) {
    seed(size, have);
    const t = { enter: [], view: [], stop: [], pause: [] };
    for (let i = 0; i < REPS; i++) {
      let s = now();
      e.fire(key, "enter", { stateMachine: "bench", taskId: `R-${n++}` });
      t.enter.push(now() - s);
      s = now();
      e.view(key);
      t.view.push(now() - s);
      s = now();
      e.stop(key, false);
      t.stop.push(now() - s);
      s = now();
      e.fire(key, "pause");
      t.pause.push(now() - s);
    }
    for (const [op, xs] of Object.entries(t)) record("wasm", size, op, xs);
  }
  e.free();
}

// --- MCP over stdio ----------------------------------------------------------
async function mcp() {
  const child = spawn(smllm, ["mcp"], { cwd: dir, env: { ...env, SMLLM_CONFIG: config } });
  const lines = createInterface({ input: child.stdout });
  const waiting = new Map();
  lines.on("line", (l) => {
    const m = JSON.parse(l);
    waiting.get(m.id)?.(m);
  });
  let id = 0;
  const meta = {
    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
    "io.modelcontextprotocol/clientCapabilities": {},
    "io.modelcontextprotocol/clientInfo": { name: "bench", version: "1" },
  };
  const tool = (args) =>
    new Promise((resolve) => {
      const n = ++id;
      waiting.set(n, (m) => resolve(m.result.content[0].text));
      child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id: n, method: "tools/call", params: { name: "smllm", arguments: args, _meta: meta } })}\n`);
    });
  const first = await tool({ event: "enter", params: { stateMachine: "bench", taskId: "M-0" } });
  const session = first.match(/session (sm-\w+)/)[1];
  await tool({ session, event: "pause" });
  let n = 1;
  const have = [1, 0];
  for (const size of SIZES) {
    for (; have[0] < size[0]; have[0]++) {
      await tool({ session, event: "enter", params: { stateMachine: "bench", taskId: `P-${n++}` } });
      await tool({ session, event: "pause" });
    }
    for (; have[1] < size[1]; have[1]++) {
      await tool({ session, event: "enter", params: { stateMachine: "bench", taskId: `C-${n++}` } });
      await tool({ session, event: "finish" });
    }
    const t = { enter: [], view: [], pause: [] };
    for (let i = 0; i < REPS; i++) {
      let s = now();
      await tool({ session, event: "enter", params: { stateMachine: "bench", taskId: `R-${n++}` } });
      t.enter.push(now() - s);
      s = now();
      await tool({ session });
      t.view.push(now() - s);
      s = now();
      await tool({ session, event: "pause" });
      t.pause.push(now() - s);
    }
    for (const [op, xs] of Object.entries(t)) record("mcp", size, op, xs);
    // The CLI measures the same store, at the same size.
    cli(size, session, () => `R-${n++}`);
  }
  child.stdin.end();
  await new Promise((r) => child.on("exit", r));
}

// --- CLI, one process per call ----------------------------------------------
function cli(size, session, ref) {
  const run = (...args) => execFileSync(smllm, [...args, "--config", config], { cwd: dir, env, input: "" });
  const t = { "fire enter": [], "session show": [], statusline: [], "fire pause": [] };
  const reps = Math.min(REPS, 10);
  for (let i = 0; i < reps; i++) {
    let s = now();
    run("fire", "enter", "--session", session, "--param", "stateMachine=bench", "--param", `taskId=${ref()}`);
    t["fire enter"].push(now() - s);
    s = now();
    run("session", "show", session);
    t["session show"].push(now() - s);
    s = now();
    run("statusline", "--session", session);
    t.statusline.push(now() - s);
    s = now();
    run("fire", "pause", "--session", session);
    t["fire pause"].push(now() - s);
  }
  for (const [op, xs] of Object.entries(t)) record("cli", size, op, xs);
}

try {
  await wasm();
  await mcp();
} finally {
  rmSync(dir, { recursive: true, force: true });
}

// --- the table ---------------------------------------------------------------
const sizes = SIZES.map(([p, c]) => `${p} / ${c}`);
console.log(`\nmedian ms per call, ${REPS} reps (cli ${Math.min(REPS, 10)}); store size = paused / completed\n`);
console.log(`| path | call | ${sizes.join(" | ")} |`);
console.log(`| ---- | ---- | ${sizes.map(() => "---:").join(" | ")} |`);
const keys = [...new Set(rows.map((r) => `${r.path}\t${r.op}`))];
for (const k of keys) {
  const [path, op] = k.split("\t");
  const cells = sizes.map((s) => rows.find((r) => r.path === path && r.op === op && r.size === s)?.ms.toFixed(3) ?? "—");
  console.log(`| ${path} | ${op} | ${cells.join(" | ")} |`);
}
