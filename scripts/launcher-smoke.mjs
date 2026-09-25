#!/usr/bin/env node
// Smoke-test the npm launcher against the real packed artifacts: `npm pack`
// the launcher and the host's platform package, extract both into a temp
// node_modules, and run the launcher end to end. This validates the packages'
// `files` manifests (a binary missing from a tarball fails here) and the
// launcher's resolve → spawn → exit-code forwarding.
//
// The launcher selects the platform package by the host's platform/arch, so
// this can only exercise the host's own package. The binary must already be
// staged at packages/<pkg>/bin/ (release CI stages it after building; locally
// copy a release build there first).

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, renameSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const require_ = createRequire(import.meta.url);
const { selectPackage, binaryName } = require_("../packages/smllm/lib/binary.js");

function fail(message) {
  console.error(`launcher-smoke: ${message}`);
  process.exit(1);
}

/** Run a command, failing loudly on spawn errors. */
function run(cmd, args, opts = {}) {
  // npm is npm.cmd on Windows; a shell resolves it.
  const shell = process.platform === "win32" && cmd === "npm";
  const r = spawnSync(cmd, args, { encoding: "utf8", shell, ...opts });
  if (r.error) {
    fail(`failed to run ${cmd}: ${r.error.message}`);
  }
  return r;
}

const pkgName = selectPackage(process.platform, process.arch);
if (!pkgName) {
  fail(`unsupported host ${process.platform}-${process.arch}`);
}
const pkgDir = join("packages", pkgName.replace("@six5536/", ""));
const binName = binaryName(process.platform);
if (!existsSync(join(pkgDir, "bin", binName))) {
  fail(
    `no staged binary at ${join(pkgDir, "bin", binName)} — ` +
      `copy a release build there first (see release.yml "Stage binary")`,
  );
}

const base = mkdtempSync(join(tmpdir(), "smllm-launcher-smoke-"));
try {
  // Pack both packages: what npm would publish, `files` manifests applied.
  const pack = run("npm", ["pack", "./packages/smllm", `./${pkgDir}`, "--pack-destination", base]);
  if (pack.status !== 0) {
    console.error(pack.stdout);
    console.error(pack.stderr);
    fail("npm pack failed");
  }
  const tarballs = pack.stdout.trim().split("\n").slice(-2);

  // Extract each tarball (root dir `package/`) into a node_modules layout.
  const nm = join(base, "node_modules");
  const dests = [join(nm, "smllm"), join(nm, pkgName)];
  for (const [i, tgz] of tarballs.entries()) {
    const scratch = join(base, `x${i}`);
    mkdirSync(scratch, { recursive: true });
    // Relative paths, cwd'd into the temp dir: GNU tar (git-bash on the
    // Windows runner) reads the drive colon in C:\... as a remote-host spec.
    const tar = run("tar", ["-xzf", tgz, "-C", `x${i}`], { cwd: base });
    if (tar.status !== 0) {
      fail(`tar failed on ${tgz}: ${tar.stderr}`);
    }
    mkdirSync(resolve(dests[i], ".."), { recursive: true });
    renameSync(join(scratch, "package"), dests[i]);
  }

  const packedBinary = join(nm, pkgName, "bin", binName);
  if (!existsSync(packedBinary)) {
    fail(`${pkgName}'s tarball does not contain bin/${binName} — check its "files" manifest`);
  }

  const launcher = join(nm, "smllm", "bin", "smllm.js");
  const version = run(process.execPath, [launcher, "--version"]);
  if (version.status !== 0 || !/^smllm \d+\.\d+\.\d+/.test(version.stdout.trim())) {
    console.error(version.stdout);
    console.error(version.stderr);
    fail(`launcher --version failed (exit ${version.status})`);
  }

  // The launcher must forward the child's exit code: an unknown flag is a
  // clap usage error, exit 2.
  const bad = run(process.execPath, [launcher, "--definitely-not-a-flag"]);
  if (bad.status !== 2) {
    fail(`launcher forwarded exit ${bad.status} for a usage error, expected 2`);
  }

  console.log(`launcher-smoke OK: ${pkgName} resolves and forwards (${version.stdout.trim()})`);
} finally {
  rmSync(base, { recursive: true, force: true });
}
