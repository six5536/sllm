// An agent loop over smllm-wasm, runtime-neutral (web-standard APIs only): it
// runs in a browser as in Node. The "model" is a script that answers
// instantly, standing in for a model that answers in milliseconds, so the
// loop can show smllm's own overhead per turn (PLAN-008 D8-19).
//
// A real host sends `tool` to its LLM as a tool definition, passes each tool
// call's arguments to `engine.callTool`, and shows the reply's text to the
// model; when the model wants to stop, it asks `engine.stop` and, on "block",
// hands the model the events list instead of stopping.

import type { Engine, ToolArgs } from "smllm-wasm";

/** One model step: call the tool, or try to stop. */
export type Step = { call: ToolArgs } | { stop: true };

/** What a run did. */
export interface Run {
  session: string;
  /** Every text smllm answered, in order. */
  texts: string[];
  turns: number;
  /** Time spent in smllm calls, in milliseconds. */
  smllmMs: number;
}

/**
 * Drive `engine` with the scripted `steps`, as a harness would. `{session}`
 * in a step's arguments is replaced by the session key.
 */
export function runAgent(engine: Engine, steps: Step[], hostSession: string, cwd: string): Run {
  let smllmMs = 0;
  const timed = <T>(f: () => T): T => {
    const t = performance.now();
    const r = f();
    smllmMs += performance.now() - t;
    return r;
  };
  // The model sees the tool first: the same one the MCP server offers.
  const tool = timed(() => engine.tool());
  const texts: string[] = [`(model gets tool ${tool.name})`];
  const bound = timed(() => engine.bind("example", hostSession, cwd));
  const session = bound.session;
  texts.push(bound.text);
  let turns = 0;
  for (const step of steps) {
    turns++;
    if ("stop" in step) {
      const decision = timed(() => engine.stop(session));
      texts.push(decision.decision === "allow" ? "(stopped)" : decision.text);
      continue;
    }
    const args = { ...step.call, session: step.call.session === "{session}" ? session : step.call.session };
    texts.push(timed(() => engine.callTool(args)).text);
  }
  return { session, texts, turns, smllmMs };
}
