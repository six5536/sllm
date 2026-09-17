# sllm

[![CI](https://github.com/six5536/sllm/actions/workflows/ci.yml/badge.svg)](https://github.com/six5536/sllm/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/sllm.svg)](https://crates.io/crates/sllm)
[![npm](https://img.shields.io/npm/v/sllm.svg)](https://www.npmjs.com/package/sllm)
[![docs.rs](https://img.shields.io/docsrs/sllm-core)](https://docs.rs/sllm-core)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

sllm is a skeleton Rust CLI. The command it ships today prints a greeting —
the point is everything around it: a two-crate workspace, a test suite with a
coverage gate, and a tag-driven release pipeline that publishes prebuilt
binaries to npm and crates.io, with man pages, completions and checksummed
archives on GitHub.

Replace `hello` with your own commands. The machinery does not change.

## Install

```sh
npm install -g sllm   # prebuilt binary, Linux/macOS/Windows
cargo install sllm    # from source, needs a Rust toolchain
```

Prebuilt binaries cover Linux and macOS on `x64` and `arm64`, and Windows on
`x64`. The Linux builds are statically linked against musl, so they need no
particular glibc version and run on Alpine too.

## Usage

```sh
sllm hello              # Hello, world!
sllm hello ada          # Hello, ada!
sllm hello ada --json   # {"name":"ada","message":"Hello, ada!"}

sllm completions zsh    # a completion script for your shell
sllm --help
sllm --version
```

Exit codes: `0` success, `2` error (a usage error exits `2` as well). Errors go
to stderr prefixed with `error: `, so stdout stays parseable when `--json` is
in play. A closed downstream pipe (`sllm man | head`) is not an error.

## Project layout

```
crates/lib/sllm-core     the library: logic, no argument parsing
crates/app/sllm          the binary: CLI parsing, wiring, output rendering
packages/sllm            the npm launcher (a JS shim that spawns the binary)
packages/sllm-<os>-<cpu> the five prebuilt-binary packages it selects from
scripts/                 version, release, and smoke-test scripts
.github/workflows/       ci.yml, checks.yml (the shared gate), release.yml, audit.yml
```

## Adding a command

1. Put the logic in `sllm-core` as a function returning `Result<T>` — no I/O,
   no printing, so it is testable without a process.
2. Add a variant to `Command` in `crates/app/sllm/src/cli.rs`, with an `Args`
   struct for its flags.
3. Wire it up in `main.rs` and render it in `output.rs` (human and `--json`).
4. Cover it: unit tests beside the code, an end-to-end test in
   `crates/app/sllm/tests/cli.rs`, and a line in `scripts/release-smoke.mjs` if
   it is part of what a shipped binary must do.

The man page and shell completions are generated from the clap definitions, so
they follow automatically.

## Development

Toolchains are pinned in `.mise.toml` and `rust-toolchain.toml` (managed with
[mise](https://mise.jdx.dev/)).

```sh
npm run build           # cargo build --workspace
npm run test            # cargo nextest run --workspace
npm run lint            # cargo clippy --workspace
npm run fmt             # cargo fmt --all
npm run coverage:check  # enforce the per-crate coverage gate (>= 90% lines)
npm run verify-version  # every version in the tree agrees
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full setup, the test layers, and
how releases are cut.

## License

MIT — see [LICENSE](LICENSE).
