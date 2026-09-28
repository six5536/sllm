// The typed layer over the raw string API (PLAN-006 D6-3): objects in and
// out, and an object-based host. Each entry module subclasses `Engine` with
// its build's wasm-bindgen `Engine` as `Raw`.

import type {
  HistoryEntry,
  Host,
  RecordKind,
  Reply,
  SessionStatus,
  State,
  Storage,
  StopDecision,
  Tool,
  ToolArgs,
} from "./types.js";

/** The host the wasm calls: strings in, strings out, every method there. */
export interface RawHost {
  supports(kind: string): boolean;
  check(kind: string, params: string, env: string, cwd: string): boolean;
  run(kind: string, params: string, env: string, cwd: string): string;
  read(file: string): string | undefined;
  isMatch(pattern: string, value: string): boolean;
  now(): number;
  random(): number;
  history(machine: string, id: string, entry: string): void;
  put(kind: RecordKind, key: string, record: string): void;
}

/** A build's wasm-bindgen `Engine`: JSON text in and out. */
export interface RawEngine {
  tool(): string;
  callTool(args: string): string;
  unsupported(): string;
  bind(harness: string, hostSession: string | null | undefined, cwd: string): string;
  view(key: string): string;
  events(key: string): string;
  status(key: string): string;
  fire(key: string | null | undefined, event: string, params: string): string;
  stop(key: string, stopHookActive: boolean): string;
  promptSubmitted(key: string): void;
  exportState(): string;
  importState(state: string): void;
  free(): void;
}

/** A build's wasm-bindgen `Engine` class. */
export type RawClass = new (compiled: string, host: RawHost) => RawEngine;

const parse = <T>(text: string): T => JSON.parse(text) as T;

/** The raw host: `host`'s methods, JSON parsed, defaults filled in. */
function adapt(host: Host): RawHost {
  return {
    supports: (kind) => Boolean(host.supports?.(kind)),
    check: (kind, params, env, cwd) =>
      host.check ? Boolean(host.check(kind, parse(params), parse(env), cwd)) : false,
    run: (kind, params, env, cwd) =>
      host.run ? (host.run(kind, parse(params), parse(env), cwd) ?? "") : `${kind} is not supported`,
    read: (file) => host.read?.(file) ?? undefined,
    isMatch: (pattern, value) =>
      host.isMatch ? host.isMatch(pattern, value) : new RegExp(pattern).test(value),
    now: () => (host.now ? host.now() : Date.now()),
    random: () => (host.random ? host.random() : crypto.getRandomValues(new Uint32Array(1))[0]!),
    history: (machine, id, entry) => {
      host.history?.(machine, id, parse<HistoryEntry>(entry));
    },
    put: (kind, key, record) => {
      host.put?.(kind, key, parse(record));
    },
  };
}

/** smllm's engine over compiled machines (`smllm compile` output). */
export class Engine {
  /** The build's raw `Engine`: set by the entry module's subclass. */
  protected static Raw: RawClass;
  #raw: RawEngine;

  /**
   * `storage`, when given, is loaded now and then receives every saved
   * record and history entry (its `put` and `history` go before `host`'s).
   */
  constructor(compiled: string | object, host: Host = {}, storage?: Storage) {
    const json = typeof compiled === "string" ? compiled : JSON.stringify(compiled);
    const Raw = (this.constructor as typeof Engine).Raw;
    const all: Host = storage
      ? {
          ...host,
          put: (kind, key, record) => {
            storage.put(kind, key, record);
            host.put?.(kind, key, record);
          },
          history: (machine, id, entry) => {
            storage.history(machine, id, entry);
            host.history?.(machine, id, entry);
          },
        }
      : host;
    this.#raw = new Raw(json, adapt(all));
    const saved = storage?.load();
    if (saved) {
      this.importState(saved);
    }
  }

  /** The agent-facing tool, to hand to an LLM's tool list (HOST-13). */
  tool(): Tool {
    return parse(this.#raw.tool());
  }

  /** Answer a tool call as the MCP server does; a malformed call is `ok: false`, never a throw. */
  callTool(args: ToolArgs = {}): Reply {
    return parse(this.#raw.callTool(JSON.stringify(args)));
  }

  /** Guard and action kinds the machines use that this host cannot run. */
  unsupported(): string[] {
    return parse(this.#raw.unsupported());
  }

  /** Bind a harness session (reused when `hostSession` is already bound). */
  bind(harness: string, hostSession?: string, cwd = ""): Reply {
    return parse(this.#raw.bind(harness, hostSession, cwd));
  }

  /** Where the session is; changes nothing. */
  view(key: string): Reply {
    return parse(this.#raw.view(key));
  }

  /** The events list of the session's state. */
  events(key: string): Reply {
    return parse(this.#raw.events(key));
  }

  status(key: string): SessionStatus {
    return parse(this.#raw.status(key));
  }

  /** Fire an event; with no key, only `enter` (it starts a session). */
  fire(key: string | undefined, event: string, params: Record<string, string> = {}): Reply {
    return parse(this.#raw.fire(key, event, JSON.stringify(params)));
  }

  stop(key: string, stopHookActive = false): StopDecision {
    return parse(this.#raw.stop(key, stopHookActive));
  }

  promptSubmitted(key: string): void {
    this.#raw.promptSubmitted(key);
  }

  exportState(): State {
    return parse(this.#raw.exportState());
  }

  importState(state: State | string): void {
    this.#raw.importState(typeof state === "string" ? state : JSON.stringify(state));
  }

  /** Free the wasm memory now, rather than when collected. */
  free(): void {
    this.#raw.free();
  }

  [Symbol.dispose](): void {
    this.free();
  }
}
