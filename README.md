# smllm

[![CI](https://github.com/six5536/smllm/actions/workflows/ci.yml/badge.svg)](https://github.com/six5536/smllm/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/smllm.svg)](https://crates.io/crates/smllm)
[![npm](https://img.shields.io/npm/v/smllm.svg)](https://www.npmjs.com/package/smllm)
[![docs.rs](https://img.shields.io/docsrs/smllm-core)](https://docs.rs/smllm-core)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

smllm puts state machines in charge of an LLM coding agent's turn loop. You describe a workflow as
a state machine in YAML: a strict subset of an [XState v5](https://stately.ai/docs/xstate) machine config.
When the agent enters a state, smllm gives it that state's instructions. When the agent tries to
stop, smllm shows the events it may fire, and the agent picks one with a single MCP tool call. A
state machine can also decide for itself: guarded transitions and eventless `always` states run
commands such as `cargo test` and choose the next state deterministically. smllm is
harness-agnostic, with Claude Code supported first.

## Features

- One MCP tool, `smllm({ session, event?, params? })`. Called with no event, it tells the agent where it is.
- Claude Code hooks: the current state is injected at session start, and stopping is blocked
  (with the events list) until the agent fires an event or `yield`s.
- Instances: each piece of work (an issue, a document, a plan) moves through a state machine. It
  gets a generated id and an optional ref that is set once, such as `GH-123`. You can park it,
  detour from it, resume it, reopen it, or have another session take it over.
- Built-in events: `enter`, `resume`, `park`, `unmatched` (a detour), and `yield`.
- Guards (`command`, `visits`) and actions (`prompt`, `command`, `setRef`), in XState's
  `{type, params}` form. Commands receive their input only through environment variables.
- `smllm validate` reports each finding as `file:line`. `smllm graph` prints text, JSON or
  Mermaid. `smllm info schema` prints a JSON Schema for editor completion.
- `smllm-wasm`: the engine for browser or Node hosts, loading `smllm compile` output.

## Install

```sh
npm install -g smllm   # prebuilt binary, Linux/macOS/Windows
cargo install smllm    # from source, needs a Rust toolchain
```

Then connect it to Claude Code, either per project or through the plugin:

```sh
smllm harness install claude          # CLAUDE.md/AGENTS.md block, .mcp.json, hooks, permission
# or
claude plugin marketplace add six5536/smllm && claude plugin install smllm@smllm
```

## Usage

```sh
smllm init                  # .smllm/config.toml
smllm new dev --write       # .smllm/dev.smllm.yaml, listed in the config
smllm validate --warnings   # findings as file:line: level: message (rule)
smllm graph dev --mermaid
```

A state machine (see [`examples/`](examples/) for the full showcase):

```yaml
id: dev
initial: TRIAGE
meta:
  smllm: 1
  instance: { noun: issue, ref: { param: issueId, pattern: "^GH-\\d+$" } }
states:
  TRIAGE:
    meta: { entryPoint: true }
    entry: { type: prompt, params: { file: triage.md } }
    on:
      issueCreated: { target: WORK, actions: setRef }
  WORK:
    on:
      submit:
        - guard: { type: command, params: { run: "cargo test --quiet" } }
          target: DONE
        - target: WORK
          reenter: true
  DONE:
    type: final
```

What the agent sees when it tries to stop in `WORK`:

```
<smllm>
session sm-k7f3q2 · dev › WORK (visit 2) · issue GH-123
Fire one event: smllm({ session: "sm-k7f3q2", event, params })
<events>
- submit
- yield — Stop for now and stay in WORK.
    note (optional): What you are waiting for.
- park — Put issue GH-123 aside and return to idle.
- unmatched — The request fits none of these; handle it from idle, then resume.
</events>
</smllm>
```

Other commands: `smllm fire --session KEY EVENT --param k=v` (the tool, from a shell),
`smllm session list|show`, `smllm instance list|show`, `smllm harness status claude`,
`smllm compile`, `smllm completions <shell>`.

Exit codes: `0` success, `1` errors found (config errors, a rejected event, a failed hook), `2`
a usage or internal error. Errors go to stderr prefixed with `error: `, and `--json` prints
exactly one object.

Config: `.smllm/config.toml` in the project, combined with `~/.config/smllm/config.toml`. When
both define the same state machine id, the project's wins. Instances live in `.smllm/state/`,
which is git-ignored. Sessions live in `~/.local/state/smllm/`.

## Project layout

```
crates/lib/smllm-core         the engine: no_std + alloc, host traits for IO (builds for wasm)
crates/lib/smllm-format       YAML/TOML loading, validation, JSON Schema, compile
crates/lib/agent-harness-kit  harness plumbing shared with sokf (install/status, hooks, reports)
crates/lib/smllm-wasm         wasm-bindgen bindings → packages/smllm-wasm (npm)
crates/app/smllm              the CLI: commands, file store, command runner, hooks, MCP server
packages/smllm*               npm launcher + prebuilt-binary packages
plugin/                       the Claude Code plugin (marketplace in .claude-plugin/)
examples/                     showcase + dev state machines
.zen/                         plan, specs (ARCHITECTURE, REQ-*, DESIGN-*), rules
```

## Development

Toolchains are pinned in `.mise.toml` and `rust-toolchain.toml` (managed with
[mise](https://mise.jdx.dev/)).

```sh
npm run build           # cargo build --workspace
npm run test            # cargo nextest run --workspace
npm run build:wasm      # packages/smllm-wasm (wasm-bindgen + wasm-opt, size budget)
npm run test:wasm       # drive the wasm engine from Node
npm run lint            # cargo clippy --workspace
npm run coverage:check  # per-crate coverage gate (>= 90% lines)
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the test layers and how releases are cut.

## License

MIT — see [LICENSE](LICENSE).
