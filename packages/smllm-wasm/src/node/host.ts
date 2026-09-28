// The Node command host (HOST-15, PLAN-008 D8-14, D8-15): `command` guards
// and actions run as the CLI runs them (DEC-4..DEC-7). Host methods are
// synchronous, so commands run with `spawnSync`. On unix the command is
// started detached, as the leader of its own process group, and a timeout
// kills that whole group, so what the command started dies with it; its
// output goes to a file, not a pipe, so a process it leaves running cannot
// hold the call open. On Windows the command's own process is killed, as the
// CLI does.
// @zen-component: HOST-JsPackage

import { type SpawnSyncOptions, spawnSync } from "node:child_process";
import { closeSync, mkdtempSync, openSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import type { Host } from "../types.js";

/** `timeoutSecs` default (DEC-5), as the CLI's. */
export const DEFAULT_TIMEOUT_SECS = 60;
const TAIL_CHARS = 400;

/** The last `TAIL_CHARS` characters of `text`, on one line (as the CLI's). */
function tail(text: string): string {
  const chars = [...text.trim()];
  const cut = chars.length > TAIL_CHARS;
  const s = chars.slice(-TAIL_CHARS).join("").split(/\s+/).filter(Boolean).join(" ");
  return cut ? `…${s}` : s;
}

/** A command's failure text, `""` when it passed. */
function runCommand(
  params: Record<string, unknown>,
  env: Record<string, string>,
  sessionCwd: string,
  configDir: string,
  timeoutSecs: number,
): string {
  const run = params.run;
  let argv: string[];
  if (typeof run === "string") {
    argv = process.platform === "win32" ? ["cmd", "/C", run] : ["sh", "-c", run];
  } else if (Array.isArray(run) && run.length > 0 && run.every((a) => typeof a === "string")) {
    argv = run;
  } else {
    return "no run param";
  }
  // `params.cwd` is relative to the compiled config (D8-22); else the
  // session's working directory, when there is one.
  const cwd =
    typeof params.cwd === "string" ? resolve(configDir, params.cwd) : sessionCwd || undefined;
  const secs = typeof params.timeoutSecs === "number" ? params.timeoutSecs : timeoutSecs;
  const unix = process.platform !== "win32";
  const dir = mkdtempSync(join(tmpdir(), "smllm-command-"));
  const out = join(dir, "out");
  const fd = openSync(out, "w");
  try {
    // `detached` is honoured by spawnSync as by spawn, though its typings
    // omit it; the test that a timed-out grandchild dies guards this.
    const options: SpawnSyncOptions & { detached: boolean } = {
      cwd,
      env: { ...process.env, ...env },
      stdio: ["ignore", fd, fd],
      timeout: secs * 1000,
      killSignal: "SIGKILL",
      // Its own session and process group (spawnSync honours it as spawn
      // does), so a timeout can kill what it started too.
      detached: unix,
      windowsVerbatimArguments: argv[0] === "cmd",
      windowsHide: true,
    };
    const r = spawnSync(argv[0]!, argv.slice(1), options);
    const timedOut = (r.error as NodeJS.ErrnoException | undefined)?.code === "ETIMEDOUT";
    if (timedOut) {
      if (unix && r.pid) {
        // The leader is gone; the rest of its group goes too. A group id is
        // not reused while members remain.
        try {
          process.kill(-r.pid, "SIGKILL");
        } catch {
          // Nothing left in the group.
        }
      }
      return `timed out after ${secs}s`;
    }
    if (r.error) {
      return `could not start: ${r.error.message}`;
    }
    return failure(r.status, r.status === 0 ? "" : readFileSync(out, "utf8"));
  } finally {
    closeSync(fd);
    rmSync(dir, { recursive: true, force: true });
  }
}

function failure(status: number | null, output: string): string {
  if (status === 0) {
    return "";
  }
  const code = status === null ? "killed by a signal" : `exited ${status}`;
  const t = tail(output);
  return t ? `${code}: ${t}` : code;
}

/**
 * A host that runs `command` guards and actions. `configDir` is the folder
 * of the compiled config: a command's `cwd` is relative to it (D8-22).
 * Spread it into your host: `new Engine(compiled, { ...nodeHost({ configDir }), ... })`.
 */
export function nodeHost(options: { configDir: string; timeoutSecs?: number }): Host {
  const { configDir, timeoutSecs = DEFAULT_TIMEOUT_SECS } = options;
  const exec = (kind: string, params: Record<string, unknown>, env: Record<string, string>, cwd: string) =>
    kind === "command" ? runCommand(params, env, cwd, configDir, timeoutSecs) : `${kind} is not supported`;
  return {
    supports: (kind) => kind === "command",
    check: (kind, params, env, cwd) => exec(kind, params, env, cwd) === "",
    run: (kind, params, env, cwd) => exec(kind, params, env, cwd),
  };
}
