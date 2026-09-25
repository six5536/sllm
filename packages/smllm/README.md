# smllm

State machines for LLM agents. You describe a workflow in YAML (an XState v5 subset), and smllm
drives a coding agent through it: it gives the agent each state's instructions, and when the
agent tries to stop, it offers the events the agent may fire through one MCP tool. Claude Code
is supported first.

## Install

```sh
npm install -g smllm
```

This package is a thin launcher. It declares a prebuilt binary for each
supported platform as an `optionalDependency`, and npm installs only the one
matching your machine; a small JS shim then runs it.

**Supported platforms:** Linux and macOS (`x64` and `arm64`), and Windows
(`x64`). On any other platform, or to build from source, use the Rust toolchain
instead:

```sh
cargo install smllm
```

## Usage

```sh
smllm init                          # .smllm/config.toml
smllm new dev --write               # scaffold a state machine
smllm validate                      # findings as file:line: level: message
smllm harness install claude        # hooks + MCP tool for Claude Code
```

Exit codes: `0` success, `2` error.

## Documentation

Full usage and development notes are in the GitHub repository:
<https://github.com/six5536/smllm#readme>.

## License

MIT — see <https://github.com/six5536/smllm/blob/main/LICENSE>.
