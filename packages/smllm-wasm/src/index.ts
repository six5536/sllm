// smllm-wasm for browsers and Node (the web build): call `init()` (or
// `initSync`) once, then use `Engine`. The string API is `smllm-wasm/raw`.
import init, { Engine as Raw, initSync } from "../wasm/web/smllm_wasm.js";
import { Engine as Base } from "./wrap.js";

export default init;
export { initSync };
export type * from "./types.js";
/** smllm's engine over compiled machines (`smllm compile` output). */
export class Engine extends Base {
  protected static override Raw = Raw;
}
