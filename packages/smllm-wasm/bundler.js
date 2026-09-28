// smllm-wasm for bundlers: the wasm loads on import. The string API is
// `smllm-wasm/raw/bundler`.
import { Engine as Raw } from "./wasm/bundler/smllm_wasm.js";
import { wrap } from "./wrap.js";

export const Engine = wrap(Raw);
