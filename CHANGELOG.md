# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
sllm uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html). While
sllm is pre-1.0, minor versions may contain breaking changes.

Every released tag needs its own section here. The release workflow refuses to
publish a version it cannot find a heading for, and that section becomes the
GitHub release notes.

## [Unreleased]

Nothing yet. Add entries as you land changes, then promote them into a
`## [X.Y.Z]` section when you cut the release.

## [0.1.0] - 2026-09-17

Initial skeleton.

### Added

- `sllm hello [NAME]`, the placeholder command: a greeting on stdout, or a JSON
  object with `--json`. A blank name is an error.
- `sllm completions <shell>` for bash, zsh, fish, PowerShell and elvish, and a
  hidden `sllm man` that renders a roff man page. Both are generated from the
  clap definitions and ship in the release archives.
- `--version`/`-V` printing `sllm X.Y.Z`, `--help` on every command, exit codes
  `0` for success and `2` for error, and a broken downstream pipe treated as a
  clean exit.
- `sllm-core`, the library half: the logic the binary drives, plus the
  `Error`/`Result` pair whose messages the CLI prints verbatim.
- The release pipeline: prebuilt binaries for Linux and macOS (`x64`/`arm64`)
  and Windows (`x64`), published to npm behind the `sllm` launcher package and
  to crates.io, with a GitHub release carrying archives, `SHA256SUMS`, the man
  page and the completions. Linux binaries are statically linked against musl.
- The CI gate, shared by `ci.yml` and `release.yml` so the two cannot drift:
  fmt, clippy with `-D warnings`, tests on macOS and Windows, doctests, docs
  with warnings as errors, the npm launcher test, version consistency across
  the tree, a per-crate line-coverage gate of 90%, and `cargo-deny` for
  licences, bans and sources. Security advisories run on a schedule instead, so
  a new RUSTSEC entry opens an issue rather than failing an unrelated PR.

[Unreleased]: https://github.com/six5536/sllm/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/six5536/sllm/releases/tag/v0.1.0
