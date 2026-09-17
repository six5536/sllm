# sllm-core

Core library for [**sllm**](https://crates.io/crates/sllm), a skeleton Rust CLI.

> This crate is the library half of the `sllm` command-line tool. Most users
> want the tool — install [`sllm`](https://crates.io/crates/sllm), not this
> crate. The library API is published mainly so the binary can depend on a
> released version, and is **not** yet considered stable.

## What it does

The binary owns argument parsing and rendering; everything it drives lives
here, so it can be tested without spawning a process. As a skeleton, that is
currently:

- **`greeting`** — the placeholder the `sllm hello` command calls.
- **`error`** — the crate's `Error`/`Result` pair, whose `Display` output the
  CLI prints verbatim after `error: `.

See the [API docs on docs.rs](https://docs.rs/sllm-core) for the full reference.

## License

MIT — see [LICENSE](https://github.com/six5536/sllm/blob/main/LICENSE).
