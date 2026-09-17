# sllm

A skeleton Rust CLI, shipped by a complete release pipeline: prebuilt binaries
for Linux, macOS and Windows, published to npm and crates.io from one tag.

## Install

```sh
npm install -g sllm
```

This package is a thin launcher. It declares a prebuilt binary for each
supported platform as an `optionalDependency`, and npm installs only the one
matching your machine; a small JS shim then runs it.

**Supported platforms:** Linux and macOS (`x64` and `arm64`), and Windows
(`x64`). On any other platform, or to build from source, use the Rust toolchain
instead:

```sh
cargo install sllm
```

## Usage

```sh
sllm hello              # Hello, world!
sllm hello ada          # Hello, ada!
sllm hello ada --json   # {"name":"ada","message":"Hello, ada!"}
sllm completions zsh    # a completion script for your shell
sllm --help
```

Exit codes: `0` success, `2` error.

## Documentation

Full usage and development notes are in the GitHub repository:
<https://github.com/six5536/sllm#readme>.

## License

MIT — see <https://github.com/six5536/sllm/blob/main/LICENSE>.
