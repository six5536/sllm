# Contributing to smllm

How to get set up, what to run before you push, and how a release is cut.

> Status: pre-1.0 alpha. Minor versions may contain breaking changes, and
> `smllm-core`'s API is not stable yet.

## Prerequisites

Toolchains are pinned and managed with [mise](https://mise.jdx.dev/):

- `rust-toolchain.toml` pins the project toolchain (`1.96.0`, with `rustfmt` and
  `clippy`).
- `.mise.toml` pins everything else: Node, a `nightly` Rust used only by the
  coverage job, `zig` (the cross C compiler behind `cargo-zigbuild`, whose
  version the release workflow reads straight out of this file), and the cargo
  tools (`cargo-nextest`, `cargo-llvm-cov`, `cargo-zigbuild`).

```sh
mise install     # install all pinned tools
npm install      # install the JS workspace (the launcher package)
```

Use `npm install`, not `npm ci`: the launcher pins the current version of every
platform package, and until a release has published that version the lockfile
cannot carry a resolved entry for it, which `npm ci` treats as fatal. That is
the case for every platform package until the first release goes out.

A plain `cargo build` needs no Node at all. Node is only needed for the npm
packages, the version scripts, and the smoke tests.

## Everyday commands

All wrapped as npm scripts (see `package.json`):

```sh
npm run build           # cargo build --workspace
npm run test            # cargo nextest run --workspace
npm run lint            # cargo clippy --workspace
npm run fmt             # cargo fmt --all
npm run check           # cargo check --workspace --tests

npm run coverage         # cargo-llvm-cov, HTML report
npm run coverage:summary # coverage summary in the terminal
npm run coverage:check   # enforce the gate: line coverage >= 90% per crate

npm run test:launcher   # node test for the npm launcher shim

npm run smoke           # behavioural smoke of a release binary (build --release first)
npm run smoke:launcher  # npm-pack the launcher + host platform package, run the
                        # real binary through it (stage the binary into
                        # packages/smllm-<host>/bin/ first)

npm run verify-version  # every version in the tree agrees (16 locations)
npm run release <ver>   # bump + verify + commit + tag (does not push)
```

Only the launcher (`packages/smllm`) is an npm workspace. The five
platform-binary packages deliberately are not: npm enforces their `os`/`cpu`
fields on workspace members unconditionally, so including them made a plain
`npm install` fail with `EBADPLATFORM` on every host. Nothing needs them to be
members. `set-version` and the release workflow address them by path.

Before opening a PR, run everything CI runs. Note that `npm run lint` is only
`cargo clippy --workspace` — CI is stricter, so use the full command here:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo test --doc --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
npm run test:launcher
npm run verify-version
npm run coverage:check     # slow; needs the nightly toolchain
```

`cargo-deny check licenses bans sources` also gates CI, but only fails when you
change dependencies.

## Documentation expectations

Documentation is gated in CI, so keep it green:

- Public items in `smllm-core` need doc comments (`#![warn(missing_docs)]`).
- `cargo doc` must build clean under `RUSTDOCFLAGS=-D warnings` (no broken
  intra-doc links, no stray HTML).
- Rustdoc examples run as doctests (`cargo test --doc`).
- **The code is the canonical reference.** The README and the CLI's `--help`
  describe actual behaviour; when a doc disagrees with the code, fix the doc.

## Tests

Tests run under `cargo-nextest`, which gives per-test process isolation. The
layers, each with a home a new command should extend:

- **Unit tests**: beside the code they cover, in a `#[cfg(test)] mod tests`.
- **Engine** (`crates/lib/smllm-core/tests/`): a fake host (`support/`) drives the protocol.
  `insta` snapshots pin every piece of agent text, and `proptest` checks the engine properties
  (ENG_P-1..4).
- **Format** (`crates/lib/smllm-format/tests/`): the examples load cleanly, the bad fixtures
  produce a snapshot of findings, and the compiled JSON round-trips.
- **CLI end-to-end** (`crates/app/smllm/tests/`): `cli.rs` covers the commands. `session.rs`
  covers scripted sessions, where a fake agent drives the Claude Code hooks, `smllm fire` and
  the MCP server over stdio (TEST-2).
- **wasm** (`packages/smllm-wasm/test/`): run `npm run build:wasm && npm run test:wasm` to drive
  one scripted session through the JS API (TEST-3).
- **Release smoke** (`scripts/release-smoke.mjs`): the same contract, run against each built
  artifact in the release workflow.
- **npm launcher** (`packages/smllm/test/`): a JS test for platform selection and exit-code
  forwarding, plus `scripts/launcher-smoke.mjs`.
- **Live model** (`scripts/live-e2e.mjs`, TEST-4): a real `claude -p` session. Run it only on
  explicit human request, with `SMLLM_LIVE=1`. It is never run in CI or hooks.

Line coverage is gated per crate at 90% (`npm run coverage:check`). Glue that
genuinely cannot be tested is marked `#[cfg_attr(coverage_nightly,
coverage(off))]`, which is why the coverage job runs on nightly.

## Project layout

See `.zen/specs/ARCHITECTURE.md`. In short:

- `crates/lib/smllm-core`: the engine. It is `no_std`: follow the WASM rules in
  `.zen/rules/rust-rules.md`, and check with
  `cargo build -p smllm-core --no-default-features --target wasm32-unknown-unknown`.
- `crates/lib/smllm-format`: parsing, validation, schema, compile.
- `crates/lib/agent-harness-kit`: harness plumbing shared with sokf (no smllm dependencies).
- `crates/lib/smllm-wasm`: wasm-bindgen bindings for `packages/smllm-wasm`.
- `crates/app/smllm`: the binary.
- `packages/`: the npm launcher, the per-platform binaries, and `smllm-wasm`.
- `plugin/`: the Claude Code plugin.
- `scripts/`: version, release, wasm build and smoke-test scripts.
- `.github/workflows/`: `ci.yml`, the shared `checks.yml` gate, `release.yml`, and the scheduled
  `audit.yml`.

## Commits and pull requests

- Use [Conventional Commits](https://www.conventionalcommits.org/) for messages
  (`feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `chore:`).
- Keep PRs focused, and update the README and `CHANGELOG.md` when behaviour
  changes.
- Make sure the full check list above passes. CI runs tests on macOS and
  Windows, and the coverage gate on Linux.

## Dependencies

Adding a dependency needs a clear reason. Reach for the standard library or a
crate already in the tree first. When you do add one, take the latest version.

## Releasing

Releases are tag-driven (`.github/workflows/release.yml`, triggered by a `v*`
tag).

**1. Write the changelog.** Add a `## [X.Y.Z]` section to `CHANGELOG.md`
(promote `[Unreleased]` if that is where the notes already are). This is not
optional. Both `npm run release` and the release workflow refuse a version they
cannot find a section for, and that section becomes the GitHub release notes.

**2. Cut the release commit and tag.**

```sh
npm run release X.Y.Z
```

That sets the version everywhere in lockstep (Cargo workspace, the internal
`smllm-core` pin, all six `package.json` files, and **both lockfiles**),
verifies it landed consistently, then commits and tags. It deliberately stops
there.

**3. Review, then push.**

```sh
git show vX.Y.Z
git push --follow-tags
```

Pushing the tag is what triggers the publish, and publishes cannot be undone
(crates.io never; npm after 72 hours).

**4. The workflow takes over**, in this order:

1. `meta` — the tag must match every version in the tree and have a changelog
   section.
2. `checks` — the full CI gate, via the shared reusable workflow.
3. `build` — build five binaries (`cargo-zigbuild` for the static-musl
   Linux targets, native `cargo` on macOS and Windows) and assert the Linux ones are static.
4. `publish` — smoke-test the binary, dry-run every publish, then publish the
   platform packages, then the launcher, then
   `cargo publish --workspace --locked`.
5. `github-release` — archives with the man page and completions, `SHA256SUMS`,
   and notes from the changelog.

A prerelease tag (`vX.Y.Z-rc.1`) publishes to npm under the `next` dist-tag and
is marked as a prerelease on GitHub, so it never becomes `latest`.

### Publishing credentials

npm uses **trusted publishing (OIDC)**, so there is no npm token. The workflow's
`id-token: write` permission is the credential, and each package has a trusted
publisher configured on npmjs.com pointing at this repository and `release.yml`.
Provenance is generated automatically as a result.

A trusted publisher can only be attached to a package that already **exists**,
so before the first release each npm package name has to be reserved by hand
with a `0.0.0` placeholder publish. Don't unpublish those afterwards: removing
a package's only version can take the package and its trusted-publisher
configuration with it.

crates.io still uses a token (`CARGO_REGISTRY_TOKEN`); it does not require a
one-time password, so it works unattended.

### Adding a platform package

A new `@six5536/smllm-<os>-<cpu>` package needs setup **before** the release
that first ships it:

1. Publish a `0.0.0` placeholder by hand (see above — one `npm publish` of the
   new package with an OTP), so the name exists.
2. Attach a trusted publisher to it on npmjs.com, pointing at this repository
   and `release.yml`. Without this the release's publish step fails.
3. Add the package to the launcher's `optionalDependencies` and the release
   workflow's build matrix and publish loops. (`verify-version` discovers
   `packages/*` itself — no change needed there.)

Until the next release publishes a real version, `npm ci` fails on the new
optional dependency (the `npm install` note under Prerequisites). That window
is expected; land the change and the release together or in quick succession.

### Version consistency

`npm run verify-version [version]` checks that the Cargo workspace, the
`smllm-core` pin, every `package.json`, the launcher's `optionalDependencies`,
`Cargo.lock` and `package-lock.json` all agree. That is 16 locations. It runs in
CI and again against the tag at release time.
