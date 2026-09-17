"use strict";

const { test } = require("node:test");
const assert = require("node:assert");
const { selectPackage, binaryName, resolveBinary, exitCode } = require("../lib/binary");

test("selectPackage maps supported platforms", () => {
  assert.strictEqual(selectPackage("linux", "x64"), "@six5536/sllm-linux-x64");
  assert.strictEqual(selectPackage("darwin", "arm64"), "@six5536/sllm-darwin-arm64");
  assert.strictEqual(selectPackage("win32", "x64"), "@six5536/sllm-win32-x64");
});

test("selectPackage returns null for unsupported platforms", () => {
  assert.strictEqual(selectPackage("win32", "arm64"), null);
  assert.strictEqual(selectPackage("linux", "riscv64"), null);
});

test("binaryName appends .exe only on Windows", () => {
  assert.strictEqual(binaryName("linux"), "sllm");
  assert.strictEqual(binaryName("win32"), "sllm.exe");
});

test("resolveBinary returns the resolved path for a supported platform", () => {
  const fakeResolve = (spec) => `/fake/node_modules/${spec}`;
  assert.strictEqual(
    resolveBinary("linux", "x64", fakeResolve),
    "/fake/node_modules/@six5536/sllm-linux-x64/bin/sllm",
  );
  // Windows resolves the .exe inside its platform package.
  assert.strictEqual(
    resolveBinary("win32", "x64", fakeResolve),
    "/fake/node_modules/@six5536/sllm-win32-x64/bin/sllm.exe",
  );
});

test("resolveBinary errors clearly on unsupported platform", () => {
  assert.throws(
    () => resolveBinary("win32", "arm64", () => "unused"),
    /No prebuilt sllm binary for win32-arm64/,
  );
});

test("exitCode forwards a normal exit status", () => {
  assert.strictEqual(exitCode({ status: 0, signal: null }), 0);
  assert.strictEqual(exitCode({ status: 2, signal: null }), 2);
});

test("exitCode encodes a signal death as 128 + signum", () => {
  const signals = { SIGINT: 2, SIGTERM: 15 };
  // Ctrl-C should surface as 130, the shell convention — not a misleading 1.
  assert.strictEqual(exitCode({ status: null, signal: "SIGINT" }, signals), 130);
  assert.strictEqual(exitCode({ status: null, signal: "SIGTERM" }, signals), 143);
});

test("exitCode falls back to 1 for an unknown signal or null status", () => {
  assert.strictEqual(exitCode({ status: null, signal: "SIGNOPE" }, {}), 1);
  assert.strictEqual(exitCode({ status: null, signal: null }), 1);
});

test("resolveBinary errors clearly when the platform package is missing", () => {
  const throwing = () => {
    throw new Error("Cannot find module");
  };
  assert.throws(
    () => resolveBinary("linux", "x64", throwing),
    /platform package "@six5536\/sllm-linux-x64" is not installed/,
  );
});
