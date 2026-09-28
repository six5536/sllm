// The Node command host runs `command` guards and actions as the CLI's runner
// does (HOST-15, DEC-4..DEC-7): the same cases as its tests in host.rs.
// @zen-test: HOST-15_AC-1
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const { nodeHost } = await import(join(here, "..", "dist/node/index.js"));

const unix = process.platform !== "win32";
const dir = realpathSync(mkdtempSync(join(tmpdir(), "smllm-node-host-")));
const host = nodeHost({ configDir: dir });
const env = { SMLLM_REF: "GH-1" };
const run = (params, cwd = "") => host.run("command", params, env, cwd);
const took = (f) => {
  const t = Date.now();
  const r = f();
  return [r, Date.now() - t];
};

test("shell, exec, env, failures and missing run", { skip: !unix }, () => {
  assert.equal(host.supports("command"), true);
  assert.equal(host.supports("x"), false);
  assert.equal(host.check("command", { run: 'test "$SMLLM_REF" = GH-1' }, env, ""), true);
  assert.equal(run({ run: "echo boom >&2; exit 3" }), "exited 3: boom");
  assert.equal(run({ run: ["true"] }), "");
  assert.match(run({ run: ["definitely-not-a-command-xyz"] }), /^could not start/);
  const big = run({ run: "head -c 1000000 /dev/zero | tr '\\0' x; echo END; exit 4" });
  assert.ok(big.startsWith("exited 4: …x") && big.endsWith("xEND"), big.slice(0, 40));
  assert.equal(run({}), "no run param");
  assert.equal(run({ run: [] }), "no run param");
  assert.equal(host.run("other", { run: "true" }, env, ""), "other is not supported");
});

test("a process left running does not hold the call", { skip: !unix }, () => {
  const [ok, fast] = took(() => run({ run: "sleep 5 & exit 0" }));
  assert.equal(ok, "");
  assert.ok(fast < 1000, `${fast} ms`);
  const [late, soon] = took(() => run({ run: "sleep 5 & echo late; exit 2" }));
  assert.equal(late, "exited 2: late");
  assert.ok(soon < 3000, `${soon} ms`);
});

test("cwd: params.cwd against the config's folder, else the session's", { skip: !unix }, () => {
  mkdirSync(join(dir, "sub"), { recursive: true });
  assert.equal(run({ run: `test "$(pwd -P)" = "${join(dir, "sub")}"`, cwd: "sub" }), "");
  assert.equal(run({ run: `test "$(pwd -P)" = "${dir}"` }, dir), "");
});

// A timeout kills the command's whole group, and only it: the grandchild
// dies, a bystander lives (as the CLI's test, D3-1).
test("a timeout kills the group and nothing else", { skip: process.platform !== "linux" }, async () => {
  const alive = (pid) => {
    try {
      const stat = readFileSync(`/proc/${pid}/stat`, "utf8");
      return !stat.slice(stat.lastIndexOf(")") + 1).trimStart().startsWith("Z");
    } catch {
      return false;
    }
  };
  const bystander = spawn("sleep", ["30"]);
  const pidfile = join(dir, "pid");
  assert.equal(run({ run: `sleep 30 & echo $! > '${pidfile}'; wait`, timeoutSecs: 1 }), "timed out after 1s");
  const grandchild = readFileSync(pidfile, "utf8").trim();
  const t = Date.now();
  while (alive(grandchild) && Date.now() - t < 3000) {
    await new Promise((r) => setTimeout(r, 20));
  }
  assert.ok(!alive(grandchild), "the group's sleep survived");
  assert.ok(alive(String(bystander.pid)), "a bystander died");
  bystander.kill();
});

test("teardown", () => {
  rmSync(dir, { recursive: true, force: true });
  assert.ok(!existsSync(dir));
});
