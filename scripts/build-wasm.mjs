#!/usr/bin/env node
// Build packages/smllm-wasm: cargo (release, wasm32, std rebuilt for size on
// a pinned nightly) → wasm-bindgen (web + bundler targets) → wasm-opt -Oz;
// print the .wasm size (NFR-8). Also regenerates the smoke test's compiled
// machines from examples/dev, and the wasm example's (examples/wasm).
//
// Needs rustup (the script installs NIGHTLY with rust-src and the wasm32
// target), wasm-bindgen-cli matching the wasm-bindgen crate version, and the
// `binaryen` and `typescript` npm devDependencies (wasm-opt, tsc).

import { execFileSync } from "node:child_process";
import { readFileSync, statSync, rmSync } from "node:fs";
import { homedir } from "node:os";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

// NFR-8: ~15% over the size measured after PLAN-007 (116.0 KiB).
const BUDGET = 134 * 1024;
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const pkg = join(root, "packages/smllm-wasm");
// std rebuilt for size, panics as a bare trap (PLAN-007 D7-3). Nightly-only
// flags, so one dated nightly, for the wasm only; the CLI stays on stable.
const NIGHTLY = "nightly-2026-09-26";
const run = (cmd, args, env) =>
  execFileSync(cmd, args, { cwd: root, stdio: "inherit", env: env && { ...process.env, ...env } });

// The package holds no path of the machine that built it (PLAN-007 D7-4):
// panic locations name crates by these prefixes, not the builder's home.
run("rustup", ["toolchain", "install", NIGHTLY, "--profile", "minimal", "--component", "rust-src", "--target", "wasm32-unknown-unknown"]);
const rustc = (...args) => execFileSync("rustc", [`+${NIGHTLY}`, ...args], { cwd: root, encoding: "utf8" }).trim();
const cargoHome = process.env.CARGO_HOME ?? join(homedir(), ".cargo");
const rustSrc = join(rustc("--print", "sysroot"), "lib/rustlib/src/rust");
const commit = rustc("-vV").match(/^commit-hash: (\S+)$/m)[1];
const rustflags = [
  `--remap-path-prefix=${cargoHome}=/cargo`,
  `--remap-path-prefix=${root}=/smllm`,
  // Where std's own paths already point (its prebuilt paths are /rustc/<commit>).
  `--remap-path-prefix=${rustSrc}=/rustc/${commit}`,
  "-Zunstable-options",
  "-Cpanic=immediate-abort",
];
// prettier-ignore
run("cargo", [
  `+${NIGHTLY}`, "build", "--release", "--locked", "-p", "smllm-wasm", "--target", "wasm32-unknown-unknown",
  "-Zbuild-std=std,panic_abort", "-Zbuild-std-features=optimize_for_size",
], {
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
// The typed layer, TypeScript over the raw build (PLAN-008 D8-18); type
// errors fail the build.
run(join(root, "node_modules/.bin/tsc"), ["-p", pkg]);
run("cargo", ["run", "--quiet", "--locked", "-p", "smllm", "--", "compile", "examples/dev/config.toml", "-o", "packages/smllm-wasm/test/dev.json"]);
run("cargo", ["run", "--quiet", "--locked", "-p", "smllm", "--", "compile", "examples/wasm/config.toml", "-o", "examples/wasm/machines.json"]);
