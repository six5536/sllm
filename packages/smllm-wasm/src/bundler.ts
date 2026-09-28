// smllm-wasm for bundlers: the wasm loads on import. The string API is
// `smllm-wasm/raw/bundler`.
import { Engine as Raw } from "../wasm/bundler/smllm_wasm.js";
import { Engine as Base } from "./wrap.js";

export type * from "./types.js";
export { memoryStorage } from "./storage.js";
/** smllm's engine over compiled machines (`smllm compile` output). */
export class Engine extends Base {
  protected static override Raw = Raw;
}
