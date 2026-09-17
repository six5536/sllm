#!/usr/bin/env node
// Behavioural smoke test for a compiled sllm binary: run the real thing and
// assert what a user sees — the version line, human and JSON output, and the
// exit codes. Release CI runs it against each built artifact its runner can
// execute; locally run `npm run smoke` after `cargo build --release -p sllm`.
//
// This deliberately mirrors crates/app/sllm/tests/cli.rs: those tests prove the
// code is right, this proves the *shipped artifact* is. Extend both together.

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";

const bin =
  process.argv[2] ??
  join("target", "release", process.platform === "win32" ? "sllm.exe" : "sllm");

function fail(message) {
  console.error(`release-smoke: ${message}`);
  process.exit(1);
}

if (!existsSync(bin)) {
  fail(`no binary at ${bin} — build first: cargo build --release -p sllm`);
}

/** Run the binary, asserting the exit status; returns the spawn result. */
function run(args, expectStatus) {
  const r = spawnSync(bin, args, { encoding: "utf8" });
  if (r.error) {
    fail(`failed to run ${bin}: ${r.error.message}`);
  }
  if (r.status !== expectStatus) {
    console.error(r.stdout);
    console.error(r.stderr);
    fail(`\`sllm ${args.join(" ")}\` exited ${r.status}, expected ${expectStatus}`);
  }
  return r;
}

// --- The version line, which the publish step also checks against the tag ---
const version = run(["--version"], 0).stdout.trim();
if (!/^sllm \d+\.\d+\.\d+/.test(version)) {
  fail(`unexpected --version output: ${version}`);
}

// --- Human output -----------------------------------------------------------
const hello = run(["hello", "ada"], 0).stdout;
if (hello !== "Hello, ada!\n") {
  fail(`unexpected hello output: ${JSON.stringify(hello)}`);
}

// --- JSON output, parsed rather than pattern-matched ------------------------
const json = run(["hello", "ada", "--json"], 0).stdout;
let doc;
try {
  doc = JSON.parse(json);
} catch (err) {
  fail(`--json output is not valid JSON (${err.message}): ${JSON.stringify(json)}`);
}
if (doc.name !== "ada" || doc.message !== "Hello, ada!") {
  fail(`unexpected --json document: ${json}`);
}

// --- Packaging surfaces: the man page and completions go into the archives ---
const man = run(["man"], 0).stdout;
if (!man.includes(".TH sllm 1") || !man.includes(".SH COMMANDS")) {
  fail("man output is missing its expected sections");
}
if (!run(["completions", "bash"], 0).stdout.includes("_sllm()")) {
  fail("bash completions are missing the _sllm function");
}

// --- Failure paths: a bad argument and a usage error both exit 2 ------------
const blank = run(["hello", "   "], 2);
if (!blank.stderr.startsWith("error: ")) {
  fail(`a failed run should explain itself on stderr: ${JSON.stringify(blank.stderr)}`);
}
run(["--definitely-not-a-flag"], 2);

console.log(`release-smoke OK: ${version} greets, renders JSON, and exits 2 on error (${bin})`);
