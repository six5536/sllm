// smllm-wasm for browsers and Node (the web build): call `init()` (or
// `initSync`) once, then use `Engine`. The string API is `smllm-wasm/raw`.
import init, { Engine as Raw, initSync } from "./wasm/web/smllm_wasm.js";
import { wrap } from "./wrap.js";

export default init;
export { initSync };
export const Engine = wrap(Raw);
