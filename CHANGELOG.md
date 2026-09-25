# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
smllm uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html). While
smllm is pre-1.0, minor versions may contain breaking changes.

Every released tag needs its own section here. The release workflow refuses to
publish a version it cannot find a heading for, and that section becomes the
GitHub release notes.

## [Unreleased]

The project is now smllm: state machines for LLM agents (PLAN-001).

### Added

- `smllm-core`: the `no_std` engine. It handles instances, idle, the built-in events (`enter`,
  `resume`, `park`, `unmatched`, `yield`), guarded transitions, `always` states, actions, visit
  counts, and the `<smllm>` agent text. The host supplies storage, commands, prompt files,
  patterns, time and randomness.
- `smllm-format`: machine files written as an XState v5 subset in YAML. It validates them with
  `file:line` findings, generates a JSON Schema, combines user and project config, and compiles
  machines to JSON.
- `agent-harness-kit`: harness plumbing factored out of sokf, covering install/status of parts,
  marker regions, JSON settings merges, hook answers, findings and CLI conventions.
- New CLI commands: `init`, `new`, `validate`, `fire`, `session`, `instance`, `harness
  install|status|hook`, `graph`, `info schema`, `compile`, and the hidden `mcp` stdio server
  with its one `smllm` tool.
- Claude Code integration: `smllm harness install claude` for a project or user, plus a Claude
  Code plugin (`plugin/`, marketplace in `.claude-plugin/`).
- `smllm-wasm` and the `smllm-wasm` npm package. CI builds the core `no_std` for wasm32 and
  enforces a 300 KiB size budget.
- `examples/showcase` (the superdev showcase, converted) and `examples/dev`.

### Changed

- Renamed the project from sllm to smllm.
- Exit codes: `1` now means errors were found (config errors, a rejected event, a failed hook).
  `2` still means a usage or internal error.

### Removed

- The `hello` placeholder command.

## [0.1.0] - 2026-09-17

Initial skeleton.

### Added

- `smllm hello [NAME]`, the placeholder command: a greeting on stdout, or a JSON
  object with `--json`. A blank name is an error.
- `smllm completions <shell>` for bash, zsh, fish, PowerShell and elvish, and a
  hidden `smllm man` that renders a roff man page. Both are generated from the
  clap definitions and ship in the release archives.
- `--version`/`-V` printing `smllm X.Y.Z`, `--help` on every command, exit codes
  `0` for success and `2` for error, and a broken downstream pipe treated as a
  clean exit.
- `smllm-core`, the library half: the logic the binary drives, plus the
  `Error`/`Result` pair whose messages the CLI prints verbatim.
- The release pipeline: prebuilt binaries for Linux and macOS (`x64`/`arm64`)
  and Windows (`x64`), published to npm behind the `smllm` launcher package and
  to crates.io, with a GitHub release carrying archives, `SHA256SUMS`, the man
  page and the completions. Linux binaries are statically linked against musl.
- The CI gate, shared by `ci.yml` and `release.yml` so the two cannot drift:
  fmt, clippy with `-D warnings`, tests on macOS and Windows, doctests, docs
  with warnings as errors, the npm launcher test, version consistency across
  the tree, a per-crate line-coverage gate of 90%, and `cargo-deny` for
  licences, bans and sources. Security advisories run on a schedule instead, so
  a new RUSTSEC entry opens an issue rather than failing an unrelated PR.

[Unreleased]: https://github.com/six5536/smllm/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/six5536/smllm/releases/tag/v0.1.0
