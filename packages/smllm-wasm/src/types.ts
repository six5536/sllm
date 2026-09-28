// Types of the typed smllm-wasm API, shared by the web and bundler entry
// points (PLAN-006 D6-3; TypeScript since PLAN-008 D8-18). They mirror what
// smllm-core serializes.

/** Where a session is after a call; all `null` in idle. */
export interface Location {
  machine: string | null;
  state: string | null;
  instance: string | null;
  ref: string | null;
}

/** The answer to a call: the `<smllm>` text for the agent, and where the session is. */
export interface Reply {
  /** `false` when the event was rejected or the instance moved. */
  ok: boolean;
  session: string;
  location: Location;
  text: string;
}

/** The stop hook's decision (TURN-4..6). */
export type StopDecision =
  | { decision: "allow" }
  /** `block`: hold the agent and show it `text`; `runaway`: let it stop, show `text` to the user. */
  | { decision: "block" | "runaway"; text: string };

export interface InstanceStatus {
  machine: string;
  kind: string;
  id: string;
  ref: string | null;
  label: string;
  status: "active" | "interrupted" | "paused" | "completed";
}

/** Where a session is, as data (the `smllm statusline --json` object). */
export interface SessionStatus {
  session: string;
  idle: boolean;
  machine: string | null;
  state: string | null;
  visit: number | null;
  yielded: boolean;
  instance: InstanceStatus | null;
  interrupted: InstanceStatus | null;
  paused: number;
}

/** One history entry, handed to `Host.history` as it happens. */
export interface HistoryEntry {
  /** Unix ms. */
  at: number;
  session: string;
  event: string;
  from: string | null;
  to: string | null;
  params: Record<string, string>;
  trace: string[];
}

/**
 * What the engine needs from its host. Every method is optional:
 * - `supports`, `check`, `run`: host guard/action kinds (`command`); none by default.
 *   A command's `params.cwd` is as written: relative to its machine file.
 * - `read`: prompt files (`smllm compile` inlines them, so rarely needed).
 * - `isMatch`: `new RegExp(pattern).test(value)` by default.
 * - `now`, `random`: `Date.now()` and `crypto.getRandomValues` by default.
 * - `history`: each history entry as it happens; dropped by default.
 * - `put`: each session, binding and instance as it is saved (write-through storage); dropped by default.
 * A method that throws is a host failure the engine reports, never a broken engine.
 */
export interface Host {
  supports?(kind: string): boolean;
  check?(kind: string, params: Record<string, unknown>, env: Record<string, string>, cwd: string): boolean;
  /** Nothing (or `""`) on success, else the failure detail. */
  run?(kind: string, params: Record<string, unknown>, env: Record<string, string>, cwd: string): string | void;
  read?(file: string): string | undefined;
  isMatch?(pattern: string, value: string): boolean;
  now?(): number;
  random?(): number;
  history?(machine: string, id: string, entry: HistoryEntry): void;
  put?(kind: RecordKind, key: string, record: unknown): void;
}

/** What `put` hands on: a session (key: its key), a binding (`harness/hostSession`, record: the session key) or an instance (`machine/id`). */
export type RecordKind = "session" | "binding" | "instance";

/**
 * Where an engine's state lives (HOST-14): `load` once, into `importState`;
 * then each saved record through `put` and each history entry through
 * `history`, as they happen, so persisting costs what changed.
 */
export interface Storage {
  load(): State | undefined;
  put(kind: RecordKind, key: string, record: unknown): void;
  history(machine: string, id: string, entry: HistoryEntry): void;
}

/** The agent-facing `smllm` tool: the same definition the MCP server offers. */
export interface Tool {
  name: string;
  description: string;
  /** JSON Schema of `ToolArgs`. */
  inputSchema: Record<string, unknown>;
}

/** The agent's tool arguments. */
export interface ToolArgs {
  session?: string;
  event?: string;
  params?: Record<string, string>;
}

/** Sessions, bindings and instances by key, as `exportState` returns them and `importState` takes them. */
export interface State {
  sessions: Record<string, unknown>;
  bindings: Record<string, string>;
  instances: Record<string, unknown>;
}
