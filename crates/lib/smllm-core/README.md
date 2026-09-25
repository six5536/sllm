# smllm-core

The engine of [**smllm**](https://crates.io/crates/smllm): state machines that drive an LLM
agent's turn loop.

`no_std` + `alloc`. It has no IO, time or randomness of its own: a host supplies them through
the traits in `smllm_core::host`: `Store`, `Guard`, `Action`, `InstructionSource`, `Matcher`,
`Clock` and `Ids`. The same engine runs in the `smllm` CLI and, through `smllm-wasm`, in a
browser.

- `Engine::bind / view / menu / fire / stop / prompt_submitted` is the whole protocol.
- `model` holds the lowered state machine. `smllm-format` builds it from YAML, and the `serde`
  feature deserialises `smllm compile` JSON.
- `record` holds sessions, instances and history entries.
- Every reply is `<smllm>` text for the agent, plus where the session ended up.

Most users want the tool: install [`smllm`](https://crates.io/crates/smllm). The API is not yet
stable. See the [API docs](https://docs.rs/smllm-core).

## License

MIT — see [LICENSE](https://github.com/six5536/smllm/blob/main/LICENSE).
