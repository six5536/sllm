#!/usr/bin/env node
// Cut a release commit and tag. Deliberately stops before pushing: the push is
// what triggers an irreversible publish, so it stays a human action.
//
// Usage: node scripts/release.mjs <version>
//
//   1. refuse a dirty working tree
//   2. refuse a version with no CHANGELOG section
//   3. set the version everywhere (including lockfiles)
//   4. verify it landed consistently
//   5. commit and tag
//
// Checks 1 and 2 run before anything is written, so a failure leaves the tree
// untouched.

import { readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const version = process.argv[2]?.replace(/^v/, "");
const root = join(dirname(fileURLToPath(import.meta.url)), "..");

if (!version || !/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error("usage: node scripts/release.mjs <semver>");
  process.exit(1);
}

const tag = `v${version}`;
const run = (cmd, args, opts = {}) =>
  execFileSync(cmd, args, { cwd: root, encoding: "utf8", ...opts });
const step = (msg) => console.log(`\n\u2192 ${msg}`);

const fail = (msg) => {
  console.error(`\nrelease aborted: ${msg}`);
  process.exit(1);
};

// --- 1. Clean working tree --------------------------------------------------
if (run("git", ["status", "--porcelain"]).trim() !== "") {
  fail("working tree is dirty; commit or stash first");
}

// --- 2. Tag must not already exist -----------------------------------------
const tags = run("git", ["tag", "--list", tag]).trim();
if (tags !== "") {
  fail(`tag ${tag} already exists`);
}

// --- 3. CHANGELOG must have a section for this version ----------------------
// This is the gate that makes "update the changelog" non-optional; the release
// workflow extracts the same section for the GitHub release notes.
const changelog = readFileSync(join(root, "CHANGELOG.md"), "utf8");
if (!new RegExp(`^## \\[${version.replace(/[.\\+]/g, "\\$&")}\\]`, "m").test(changelog)) {
  fail(
    `CHANGELOG.md has no "## [${version}]" section.\n` +
      "  Add one (promote [Unreleased] if that is where the notes are) and retry.",
  );
}

// --- 4. Set the version everywhere ------------------------------------------
step(`setting version to ${version}`);
run("node", [join(root, "scripts/set-version.mjs"), version], { stdio: "inherit" });

// --- 5. Verify it landed consistently ---------------------------------------
step("verifying version consistency");
run("node", [join(root, "scripts/verify-version.mjs"), version], { stdio: "inherit" });

// --- 6. Commit and tag ------------------------------------------------------
step("committing and tagging");
run("git", ["add", "-A"], { stdio: "inherit" });
run("git", ["commit", "-m", `chore(release): ${tag}`], { stdio: "inherit" });
run("git", ["tag", "-a", tag, "-m", tag], { stdio: "inherit" });

console.log(`
Prepared ${tag}.

  Review:  git show ${tag}
  Publish: git push --follow-tags

Pushing the tag triggers the release workflow, which publishes to npm and
crates.io. Those publishes cannot be undone.`);
