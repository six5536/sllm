#!/usr/bin/env node
// Build packages/smllm-wasm: cargo (release, wasm32) → wasm-bindgen (web +
// bundler targets) → wasm-opt -Oz; print the .wasm size (NFR-8). Also
// regenerates the smoke test's compiled machines from examples/dev.
//
// Needs wasm-bindgen-cli matching the wasm-bindgen crate version, and the
// `binaryen` npm devDependency for wasm-opt.

import { execFileSync } from "node:child_process";
import { readFileSync, statSync, rmSync } from "node:fs";
import { homedir } from "node:os";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

// NFR-8: set after P3 at ~15% over the first measured size (258 KiB).
const BUDGET = 300 * 1024;
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const pkg = join(root, "packages/smllm-wasm");
const run = (cmd, args, env) =>
  execFileSync(cmd, args, { cwd: root, stdio: "inherit", env: env && { ...process.env, ...env } });

// The package holds no path of the machine that built it (PLAN-007 D7-4):
// panic locations name crates by these prefixes, not the builder's home.
const rustc = (...args) => execFileSync("rustc", args, { cwd: root, encoding: "utf8" }).trim();
const cargoHome = process.env.CARGO_HOME ?? join(homedir(), ".cargo");
const rustSrc = join(rustc("--print", "sysroot"), "lib/rustlib/src/rust");
const commit = rustc("-vV").match(/^commit-hash: (\S+)$/m)[1];
const rustflags = [
  `--remap-path-prefix=${cargoHome}=/cargo`,
  `--remap-path-prefix=${root}=/smllm`,
  // Where std's own paths already point (its prebuilt paths are /rustc/<commit>).
  `--remap-path-prefix=${rustSrc}=/rustc/${commit}`,
];
run("cargo", ["build", "--release", "--locked", "-p", "smllm-wasm", "--target", "wasm32-unknown-unknown"], {
  CARGO_ENCODED_RUSTFLAGS: rustflags.join("\x1f"),
});
const raw = join(root, "target/wasm32-unknown-unknown/release/smllm_wasm.wasm");
for (const target of ["web", "bundler"]) {
  const out = join(pkg, "wasm", target);
  rmSync(out, { recursive: true, force: true });
  run("wasm-bindgen", ["--target", target, "--out-dir", out, raw]);
  const wasm = join(out, "smllm_wasm_bg.wasm");
  // The producers and target-features sections name the toolchain; nothing reads them.
  run(join(root, "node_modules/.bin/wasm-opt"), ["-Oz", "--strip-producers", "--strip-target-features", "--enable-bulk-memory", "--enable-nontrapping-float-to-int", "--enable-sign-ext", "--enable-mutable-globals", "-o", wasm, wasm]);
  if (target === "web") {
    const bytes = statSync(wasm).size;
    console.log(`smllm_wasm_bg.wasm: ${bytes} bytes (${(bytes / 1024).toFixed(1)} KiB, budget ${BUDGET / 1024} KiB)`);
    if (bytes > BUDGET) {
      console.error(`error: smllm_wasm_bg.wasm is over its ${BUDGET / 1024} KiB budget (NFR-8)`);
      process.exit(1);
    }
    const text = readFileSync(wasm, "latin1");
    for (const path of [homedir(), cargoHome, root, rustSrc]) {
      if (text.includes(path)) {
        console.error(`error: smllm_wasm_bg.wasm holds the build path ${path} (PLAN-007 D7-4)`);
        process.exit(1);
      }
    }
  }
}
run("cargo", ["run", "--quiet", "--locked", "-p", "smllm", "--", "compile", "examples/dev/config.toml", "-o", "packages/smllm-wasm/test/dev.json"]);
