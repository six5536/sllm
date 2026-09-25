# smllm-format

Reads, checks and compiles [**smllm**](https://crates.io/crates/smllm) state machine files.

A machine file is an XState v5 machine config written in YAML, a strict subset, with smllm's
own data under `meta`.

- `load_machine` parses and validates a file. It collects every finding (error, warning or
  info), each with its file, line, YAML path, fix hint and rule id, and lowers the file to the
  `smllm-core` model.
- `load_configs` combines the user and project `config.toml` files. The project wins when both
  define the same machine id.
- `json_schema` gives the format's JSON Schema, and `compile` gives the model as JSON for
  `smllm-wasm` hosts.

## License

MIT — see [LICENSE](https://github.com/six5536/smllm/blob/main/LICENSE).
