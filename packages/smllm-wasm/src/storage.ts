// In-memory storage (HOST-14): what `nodeFileStorage` does with files,
// without them; for tests, examples and hosts that persist elsewhere.
// @zen-component: HOST-JsPackage

import type { HistoryEntry, RecordKind, State, Storage } from "./types.js";

/** A storage that keeps everything in memory. `history` holds the log by `machine/id`. */
export function memoryStorage(): Storage & { readonly log: Map<string, HistoryEntry[]> } {
  const state: State = { sessions: {}, bindings: {}, instances: {} };
  const log = new Map<string, HistoryEntry[]>();
  let empty = true;
  const shelf: Record<RecordKind, Record<string, unknown>> = {
    session: state.sessions,
    binding: state.bindings,
    instance: state.instances,
  };
  return {
    load: () => (empty ? undefined : structuredClone(state)),
    put(kind, key, record) {
      shelf[kind][key] = record;
      empty = false;
    },
    history(machine, id, entry) {
      const key = `${machine}/${id}`;
      log.set(key, [...(log.get(key) ?? []), entry]);
    },
    log,
  };
}
