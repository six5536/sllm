#!/usr/bin/env node
// Build packages/smllm-wasm: cargo (release, wasm32) → wasm-bindgen (web +
// bundler targets) → wasm-opt -Oz; print the .wasm size (NFR-8). Also
// regenerates the smoke test's compiled machines from examples/dev.
//
// Needs wasm-bindgen-cli matching the wasm-bindgen crate version, and the
// `binaryen` npm devDependency for wasm-opt.

import { execFileSync } from "node:child_process";
import { statSync, rmSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

// NFR-8: set after P3 at ~15% over the first measured size (258 KiB).
const BUDGET = 300 * 1024;
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const pkg = join(root, "packages/smllm-wasm");
const run = (cmd, args) => execFileSync(cmd, args, { cwd: root, stdio: "inherit" });

run("cargo", ["build", "--release", "--locked", "-p", "smllm-wasm", "--target", "wasm32-unknown-unknown"]);
const raw = join(root, "target/wasm32-unknown-unknown/release/smllm_wasm.wasm");
for (const target of ["web", "bundler"]) {
  const out = join(pkg, "wasm", target);
  rmSync(out, { recursive: true, force: true });
  run("wasm-bindgen", ["--target", target, "--out-dir", out, raw]);
  const wasm = join(out, "smllm_wasm_bg.wasm");
  run(join(root, "node_modules/.bin/wasm-opt"), ["-Oz", "--enable-bulk-memory", "--enable-nontrapping-float-to-int", "--enable-sign-ext", "--enable-mutable-globals", "-o", wasm, wasm]);
  if (target === "web") {
    const bytes = statSync(wasm).size;
    console.log(`smllm_wasm_bg.wasm: ${bytes} bytes (${(bytes / 1024).toFixed(1)} KiB, budget ${BUDGET / 1024} KiB)`);
    if (bytes > BUDGET) {
      console.error(`error: smllm_wasm_bg.wasm is over its ${BUDGET / 1024} KiB budget (NFR-8)`);
      process.exit(1);
    }
  }
}
run("cargo", ["run", "--quiet", "--locked", "-p", "smllm", "--", "compile", "examples/dev/config.toml", "-o", "packages/smllm-wasm/test/dev.json"]);
