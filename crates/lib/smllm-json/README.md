# smllm-json

A small `no_std` JSON reader and writer, built for [**smllm**](https://crates.io/crates/smllm)'s
WebAssembly engine, where every kilobyte counts.

- `parse` reads RFC 8259 JSON into a `JsonValue` tree: strict grammar, a nesting limit, and no
  float code (integers are `i64` / `u64`; any other number is kept as its text).
- `FromJson` and `Fields` read typed values out of the tree with serde's rules for struct fields;
  errors name the field that failed (`machines[0].id: expected a string, found an integer`).
- `ToJson` and `object` write JSON escaped exactly as serde_json does, so the output is
  byte-identical to serde_json's for the same shapes.

It has no dependencies.

## License

MIT — see [LICENSE](https://github.com/six5536/smllm/blob/main/LICENSE).
