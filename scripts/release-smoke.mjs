#!/usr/bin/env node
// Behavioural smoke test for a compiled smllm binary: run the real thing and
// assert what a user sees — the version line, a real config, JSON output and
// the exit codes. Release CI runs it against each built artifact its runner can
// execute; locally run `npm run smoke` after `cargo build --release -p smllm`.
//
// This deliberately mirrors crates/app/smllm/tests/cli.rs: those tests prove the
// code is right, this proves the *shipped artifact* is. Extend both together.

import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const bin = resolve(
  process.argv[2] ??
    join("target", "release", process.platform === "win32" ? "smllm.exe" : "smllm"),
);

function fail(message) {
  console.error(`release-smoke: ${message}`);
  process.exit(1);
}

if (!existsSync(bin)) {
  fail(`no binary at ${bin} — build first: cargo build --release -p smllm`);
}

/** Run the binary, asserting the exit status; returns the spawn result. */
function run(args, expectStatus, opts = {}) {
  const r = spawnSync(bin, args, { encoding: "utf8", ...opts });
  if (r.error) {
    fail(`failed to run ${bin}: ${r.error.message}`);
  }
  if (r.status !== expectStatus) {
    console.error(r.stdout);
    console.error(r.stderr);
    fail(`\`smllm ${args.join(" ")}\` exited ${r.status}, expected ${expectStatus}`);
  }
  return r;
}

// --- The version line, which the publish step also checks against the tag ---
const version = run(["--version"], 0).stdout.trim();
if (!/^smllm \d+\.\d+\.\d+/.test(version)) {
  fail(`unexpected --version output: ${version}`);
}

// --- A real config: init, scaffold, validate, fire -------------------------
// Isolated user dirs so the smoke run never touches the runner's home.
const tmp = mkdtempSync(join(tmpdir(), "smllm-smoke-"));
const env = {
  ...process.env,
  HOME: join(tmp, "home"),
  XDG_CONFIG_HOME: join(tmp, "config"),
  XDG_STATE_HOME: join(tmp, "state"),
  APPDATA: join(tmp, "appdata"),
  LOCALAPPDATA: join(tmp, "localappdata"),
};
const runIn = (args, expectStatus) => run(args, expectStatus, { cwd: tmp, env });
runIn(["init"], 0);
runIn(["new", "demo", "--write"], 0);
const report = runIn(["validate"], 0).stdout;
if (report !== "0 errors, 0 warnings, 0 info\n") {
  fail(`unexpected validate output: ${JSON.stringify(report)}`);
}

// --- JSON output, parsed rather than pattern-matched ------------------------
const json = runIn(["fire", "enter", "--param", "stateMachine=demo", "--json"], 0).stdout;
let doc;
try {
  doc = JSON.parse(json);
} catch (err) {
  fail(`--json output is not valid JSON (${err.message}): ${JSON.stringify(json)}`);
}
if (doc.ok !== true || doc.location.state !== "START" || !doc.session.startsWith("sm-")) {
  fail(`unexpected --json document: ${json}`);
}
// A rejected event exits 1.
runIn(["fire", "--session", doc.session, "bogus"], 1);
const schema = JSON.parse(run(["info", "schema"], 0).stdout);
if (schema.title !== "smllm state machine") {
  fail("info schema is not the state machine schema");
}

// --- Packaging surfaces: the man page and completions go into the archives ---
const man = run(["man"], 0).stdout;
if (!man.includes(".TH smllm 1") || !man.includes(".SH COMMANDS")) {
  fail("man output is missing its expected sections");
}
if (!run(["completions", "bash"], 0).stdout.includes("_smllm()")) {
  fail("bash completions are missing the _smllm function");
}

// --- Failure paths: a bad argument and a usage error both exit 2 ------------
const unknown = runIn(["fire", "--session", "sm-nope", "park"], 2);
if (!unknown.stderr.startsWith("error: ")) {
  fail(`a failed run should explain itself on stderr: ${JSON.stringify(unknown.stderr)}`);
}
run(["--definitely-not-a-flag"], 2);

console.log(`release-smoke OK: ${version} validates, fires, renders JSON, exits 1 on a rejected event and 2 on error (${bin})`);
