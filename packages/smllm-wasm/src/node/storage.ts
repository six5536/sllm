// Node file storage (HOST-14, PLAN-008 D8-16): a file per record, written
// atomically (temp file, then rename), and history as JSONL, one line per
// entry in the CLI's format. One process owns a folder: several processes
// sharing state should use the CLI, whose store locks.
// @zen-component: HOST-JsPackage

import { appendFileSync, mkdirSync, readdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import type { HistoryEntry, RecordKind, State, Storage } from "../types.js";

const SHELF: Record<RecordKind, keyof State> = {
  session: "sessions",
  binding: "bindings",
  instance: "instances",
};

/** Longest file name before `.json`; longer ones are cut and end in a hash. */
const MAX_NAME = 120;

/**
 * A file name for `key`: `[a-z0-9-]` kept, every other byte %-escaped (upper
 * case too, so a case-insensitive file system cannot merge two keys), cut
 * with an FNV-1a hash when long (PLAN-008 DC-4).
 */
export function fileName(key: string): string {
  let name = "";
  for (const b of new TextEncoder().encode(key)) {
    const c = String.fromCharCode(b);
    name += /[a-z0-9-]/.test(c) ? c : `%${b.toString(16).padStart(2, "0")}`;
  }
  if (name.length > MAX_NAME) {
    let h = 0xcbf29ce484222325n;
    for (const b of new TextEncoder().encode(key)) {
      h = ((h ^ BigInt(b)) * 0x100000001b3n) & 0xffffffffffffffffn;
    }
    name = `${name.slice(0, MAX_NAME - 17)}~${h.toString(16).padStart(16, "0")}`;
  }
  return name;
}

let written = 0;

/** Write `text` to `path` whole or not at all. */
function writeAtomic(path: string, text: string): void {
  const temp = `${path}.${process.pid}.${written++}.tmp`;
  writeFileSync(temp, text);
  renameSync(temp, path);
}

/** Storage in `dir`: `sessions/`, `bindings/`, `instances/` and `history/`. */
export function nodeFileStorage(dir: string): Storage {
  return {
    load() {
      const state: State = { sessions: {}, bindings: {}, instances: {} };
      let any = false;
      for (const shelf of Object.values(SHELF)) {
        let names: string[];
        try {
          names = readdirSync(join(dir, shelf)).filter((n) => n.endsWith(".json"));
        } catch {
          continue;
        }
        for (const n of names) {
          const { key, record } = JSON.parse(readFileSync(join(dir, shelf, n), "utf8"));
          (state[shelf] as Record<string, unknown>)[key] = record;
          any = true;
        }
      }
      return any ? state : undefined;
    },
    put(kind, key, record) {
      const shelf = join(dir, SHELF[kind]);
      mkdirSync(shelf, { recursive: true });
      writeAtomic(join(shelf, `${fileName(key)}.json`), JSON.stringify({ key, record }));
    },
    history(machine: string, id: string, entry: HistoryEntry) {
      const folder = join(dir, "history", fileName(machine));
      mkdirSync(folder, { recursive: true });
      appendFileSync(join(folder, `${fileName(id)}.jsonl`), `${JSON.stringify(entry)}\n`);
    },
  };
}
