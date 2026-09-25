# Requirements Specification

## Introduction

The `smllm` command line: its commands and the output conventions shared with sokf (D6: sokf's output conventions, not its repository ones). Source: PLAN-001 §9 (CLI-1..13 and the conventions table). The state machine format is in the CFG requirements; the tool and hook semantics in HOST and TURN.

## Glossary

- CONFIG: a `config.toml` listing state machine files; project config `.smllm/config.toml`, user config `~/.config/smllm/config.toml`
- FINDING: a validation result at level error, warning or info
- EXPLICIT CONFIG: a config named by `--config` or `SMLLM_CONFIG`

## Stakeholders

- AGENT USER: sets up configs and inspects sessions and instances
- SHELL-ONLY HARNESS: drives smllm through `smllm fire`
- SCRIPT AUTHOR: consumes `--json` output and exit codes

## Requirements

### CLI-1: init [MUST]

AS AN agent user, I WANT `smllm init [--user] [--json]`, SO THAT I get a starting config.

ACCEPTANCE CRITERIA

- [x] CLI-1_AC-1 [event]: WHEN the user runs `smllm init` THEN the system SHALL create `.smllm/config.toml` in the working directory (with `--user`, the user config) from the template, and SHALL leave an existing file unchanged and say so

### CLI-2: new [MUST]

AS AN agent user, I WANT `smllm new <ID> [--dir DIR] [--write] [--json]`, SO THAT I can scaffold a state machine.

ACCEPTANCE CRITERIA

- [x] CLI-2_AC-1 [event]: WHEN the user runs `smllm new <ID>` THEN the system SHALL print a `<ID>.smllm.yaml` template; with `--write` it SHALL write the file (refusing to overwrite) and add it to the config's `[machines] files`, keeping the config's formatting

### CLI-3: validate and config lookup [MUST]

AS AN agent user, I WANT `smllm validate [PATHS] [--json] [--warnings] [--info]`, SO THAT I can check configs, state machines and saved instances.

ACCEPTANCE CRITERIA

- [x] CLI-3_AC-1 [event]: WHEN the user runs `smllm validate` THEN the system SHALL check the configs found (or each given `.toml` config or machine file), their state machines and saved instances (IDLE-4), print findings per the conventions, and exit 1 when any error is found
- [x] CLI-3_AC-2 [complex]: WHEN configs are looked up THEN the system SHALL use only the explicit config if one is given; otherwise it SHALL use the user config (if present) followed by the nearest `.smllm/config.toml` at or above the working directory, and an empty set when neither exists — combining rules (project wins on `id`, project `[idle]` replaces user's) are CFG's

### CLI-4: fire [MUST]

AS A shell-only harness, I WANT `smllm fire --session KEY <EVENT> [--param KEY=VALUE]… [--json]`, SO THAT I can call the tool without MCP.

ACCEPTANCE CRITERIA

- [x] CLI-4_AC-1 [event]: WHEN the user runs `smllm fire` THEN the system SHALL fire the event with the params on the session and print the reply (text, or the reply object with `--json`), exiting 1 when the event is rejected — `--session` may be omitted only with `enter` (HOST-3)

### CLI-5: session [MUST]

AS AN agent user, I WANT `smllm session list|show <KEY> [--json]`, SO THAT I can see my sessions.

ACCEPTANCE CRITERIA

- [x] CLI-5_AC-1 [event]: WHEN the user runs `smllm session list` THEN the system SHALL list sessions newest first; `show <KEY>` SHALL print the tool's no-event view (ENG-5)

### CLI-6: instance [MUST]

AS AN agent user, I WANT `smllm instance list|show <ID> [--json]`, SO THAT I can see instances and their history.

ACCEPTANCE CRITERIA

- [x] CLI-6_AC-1 [event]: WHEN the user runs `smllm instance list` THEN the system SHALL list the configured machines' instances with state, status and holder; `show <ID>` SHALL accept an id or ref and print the instance with its history

### CLI-7: harness install|status [MUST]

AS AN agent user, I WANT `smllm harness install|status <NAME> [--scope S] [--without PART]… [--force] [--json]`, SO THAT I can integrate and inspect a harness.

ACCEPTANCE CRITERIA

- [x] CLI-7_AC-1 [event]: WHEN the user runs `smllm harness install|status claude` THEN the system SHALL install, or report the state of, each part (HOST-6) for the scope, honouring `--without` and `--force`

### CLI-8: harness hook (hidden) [MUST]

AS A harness, I WANT `smllm harness hook <NAME> <HOOK>`, SO THAT my hooks reach smllm.

ACCEPTANCE CRITERIA

- [x] CLI-8_AC-1 [event]: WHEN the harness runs `smllm harness hook claude <HOOK>` THEN the system SHALL answer that hook from stdin JSON (HOST-7); the command SHALL be hidden from help

### CLI-9: mcp (hidden) [MUST]

AS AN MCP harness, I WANT `smllm mcp`, SO THAT I get the one tool over stdio.

ACCEPTANCE CRITERIA

- [x] CLI-9_AC-1 [event]: WHEN the harness runs `smllm mcp` THEN the system SHALL serve a stdio MCP server with exactly one tool, `smllm`; the command SHALL be hidden from help

### CLI-10: graph [SHOULD]

AS AN agent user, I WANT `smllm graph [ID] [--json|--mermaid]`, SO THAT I can see states and transitions.

ACCEPTANCE CRITERIA

- [x] CLI-10_AC-1 [event]: WHEN the user runs `smllm graph` THEN the system SHALL print each (or the named) machine's states and transitions with guards shown, as text, JSON or a Mermaid `stateDiagram-v2`

### CLI-11: info schema [MUST]

AS A state machine author, I WANT `smllm info schema`, SO THAT my editor can validate files.

ACCEPTANCE CRITERIA

- [x] CLI-11_AC-1 [event]: WHEN the user runs `smllm info schema` THEN the system SHALL print the JSON Schema of the state machine format (CFG-15)

### CLI-12: completions and man [SHOULD]

AS AN agent user, I WANT shell completions and a man page, SO THAT the CLI is discoverable.

ACCEPTANCE CRITERIA

- [x] CLI-12_AC-1 [event]: WHEN the user runs `smllm completions <SHELL>` or the hidden `smllm man` THEN the system SHALL print the completion script or a roff man page (with COMMANDS, EXIT STATUS and FILES sections)

### CLI-13: compile [MUST]

AS A wasm host author, I WANT `smllm compile <FILE> [-o OUT]`, SO THAT a browser can load validated machines.

ACCEPTANCE CRITERIA

- [x] CLI-13_AC-1 [event]: WHEN the user runs `smllm compile <FILE>` THEN the system SHALL emit the validated model as compact JSON (to OUT with `-o`), or print findings on stderr and exit 1 when it has errors

### CLI-14: statusline [MUST]

AS AN agent user, I WANT `smllm statusline [--session KEY] [--json] [--color WHEN]`, SO THAT my status line shows smllm's state.

ACCEPTANCE CRITERIA

- [ ] CLI-14_AC-1 [event]: WHEN the user runs `smllm statusline` THEN the system SHALL behave as REQ-STL (STL-1..7), always exiting 0

## Assumptions

- A home directory can be resolved (XDG on Linux and macOS, Known Folders on Windows); `$XDG_CONFIG_HOME` and `$XDG_STATE_HOME` are honoured

## Constraints

- Global options: `--config <FILE>` and `-V/--version` (prints `smllm x.y.z`), accepted before or after the command
- Config lookup: CLI-3_AC-2; a session records the configs it was created with (STO-2)
- Exit codes: 0 no error; 1 errors found (config errors, a rejected event, a failed hook); 2 usage or internal error
- Output: `error: <message>` on stderr; `--json` prints one object and nothing else, with a published schema (schemas not yet published)
- Findings: `<path>:<line>: <level>: <message> (<authority>)`, counts at the end; `--warnings` / `--info` list those levels
- Tool config: `config.toml` with kebab-case keys; `harness.toml` beside it (the harness record)

## Out of Scope

- Repository conventions of sokf (smllm is not tied to a repository)
- A headless `smllm run` agent driver

## Change Log

- 0.1.0 (2026-09-25): Initial requirements from PLAN-001 §9
- 0.2.0 (2026-09-25): CLI-14 statusline (PLAN-002)
