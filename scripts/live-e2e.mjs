#!/usr/bin/env node
// Live-model end-to-end test (TEST-4): a real Claude Code session, driven by
// smllm's hooks and MCP tool, walks a two-state machine to its final state.
//
// ONLY RUN ON EXPLICIT HUMAN REQUEST. It spends model tokens, needs a logged-in
// `claude` CLI, and is never part of CI or any hook.
//
// Usage: node scripts/live-e2e.mjs [path/to/smllm]   (default target/release/smllm)

import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync, mkdirSync, readdirSync, readFileSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

if (process.env.SMLLM_LIVE !== "1") {
  console.error("live-e2e: refusing to run without SMLLM_LIVE=1 (it calls a real model; human request only)");
  process.exit(2);
}

const bin = resolve(process.argv[2] ?? join("target", "release", "smllm"));
const tmp = mkdtempSync(join(tmpdir(), "smllm-live-"));
const project = join(tmp, "project");
const binDir = join(tmp, "bin");
mkdirSync(join(project, ".smllm"), { recursive: true });
mkdirSync(binDir);
symlinkSync(bin, join(binDir, "smllm"));
const env = { ...process.env, PATH: `${binDir}:${process.env.PATH}`, XDG_STATE_HOME: join(tmp, "state") };

writeFileSync(
  join(project, ".smllm", "config.toml"),
  '[machines]\nfiles = ["hello.smllm.yaml"]\n',
);
writeFileSync(
  join(project, ".smllm", "hello.smllm.yaml"),
  `id: hello
description: Write hello.txt, then finish.
initial: WRITE
meta: { smllm: 1 }
states:
  WRITE:
    meta: { entryPoint: true }
    entry: { type: prompt, params: { text: "Create hello.txt containing the word hello, then fire written." } }
    on:
      written: CHECK
  CHECK:
    always:
      - guard: { type: command, params: { run: "grep -q hello hello.txt" } }
        target: DONE
      - target: WRITE
  DONE:
    type: final
    entry: { type: prompt, params: { text: "Say that you are done." } }
`,
);

const run = (cmd, args, opts = {}) => {
  const r = spawnSync(cmd, args, { cwd: project, env, encoding: "utf8", ...opts });
  if (r.status !== 0) {
    console.error(r.stdout, r.stderr);
    throw new Error(`${cmd} ${args.join(" ")} exited ${r.status}`);
  }
  return r.stdout;
};

run("git", ["init", "-q"]);
run(bin, ["harness", "install", "claude"]);
const out = run(
  "claude",
  [
    "-p",
    "Enter the hello state machine with smllm and follow its instructions until it completes.",
    "--permission-mode",
    "acceptEdits",
    "--allowedTools",
    "mcp__smllm__smllm,Write,Edit,Read,Bash",
  ],
  { timeout: 10 * 60 * 1000 },
);
console.log(out);

// The history path: enter → WRITE, written → DONE (via CHECK).
const dir = join(project, ".smllm", "state", "hello");
const inst = readdirSync(dir).find((f) => f.endsWith(".json"));
const state = JSON.parse(readFileSync(join(dir, inst), "utf8"));
const history = readFileSync(join(dir, inst.replace(/\.json$/, ".history.jsonl")), "utf8")
  .trim()
  .split("\n")
  .map((l) => JSON.parse(l));
const events = history.map((h) => h.event);
if (state.status !== "completed" || !events.includes("enter") || !events.includes("written")) {
  console.error(JSON.stringify({ state, events }, null, 2));
  console.error("live-e2e: FAILED");
  process.exit(1);
}
console.log(`live-e2e OK: ${events.join(" → ")} (${tmp})`);
