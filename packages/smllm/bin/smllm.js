#!/usr/bin/env node
"use strict";

// Thin launcher: resolve the prebuilt binary for this platform and run it,
// forwarding arguments, stdio, and the exit code.

const { spawnSync } = require("node:child_process");
const { resolveBinary, exitCode } = require("../lib/binary");

let binary;
try {
  binary = resolveBinary(process.platform, process.arch);
} catch (err) {
  process.stderr.write(`${err.message}\n`);
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });

if (result.error) {
  process.stderr.write(`failed to run smllm: ${result.error.message}\n`);
  process.exit(1);
}

process.exit(exitCode(result));
