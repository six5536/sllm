# Requirements Specification

## Introduction

smllm in the harness's status bar: `smllm statusline` prints where a session is (state machine, state, instance) as a coloured row or as JSON, for the user's status line command to include. A skill adds it to the user's Claude Code status line. Source: PLAN-002 (decisions D2-1..D2-11).

## Glossary

- STATUS LINE: Claude Code's `statusLine` setting: one command whose stdout rows are shown under the prompt; it gets the status JSON (`session_id`, `cwd`, `model`, …) on stdin
- ROW: one line of status line output
- STATUS: where a session is, as the fields of `--json` (STL-5)

## Stakeholders

- AGENT USER: wants to see smllm's state while the agent works
- STATUS LINE AUTHOR: shapes their own row from the JSON
- HARNESS AUTHOR: shows smllm's state in another harness or a wasm host

## Requirements

### STL-1: statusline command [MUST]

AS AN agent user, I WANT `smllm statusline [--session KEY] [--json] [--color auto|always|never]`, SO THAT my status line can show smllm's state.

ACCEPTANCE CRITERIA

- [x] STL-1_AC-1 [event]: WHEN the user runs `smllm statusline` THEN the system SHALL print the session's default row (STL-6), or with `--json` its status object (STL-5)

### STL-2: session lookup [MUST]

AS AN agent user, I WANT the row to follow the Claude Code session it is shown in, SO THAT I set it up once.

ACCEPTANCE CRITERIA

- [x] STL-2_AC-1 [event]: WHEN `--session KEY` is given THEN the system SHALL show that session; otherwise it SHALL read the status JSON on stdin and show the session bound to its `session_id` (HOST-4)
- [x] STL-2_AC-2 [conditional]: IF there is no session to show (no stdin `session_id`, no binding, unknown key) THEN the system SHALL print nothing (with `--json`, `{}`) and exit 0 — D2-3

### STL-3: read-only and fast [MUST]

AS AN agent user, I WANT the status line to cost nothing, SO THAT it can run after every message.

ACCEPTANCE CRITERIA

- [x] STL-3_AC-1 [ubiquitous]: The system SHALL NOT write to the store, bind a session or run guard or action commands while answering `statusline`
- [ ] STL-3_AC-2 [ubiquitous]: The system SHOULD answer within 20 ms on a typical project (NFR-2)

### STL-4: never blanks the status line [MUST]

AS AN agent user, I WANT a broken smllm setup to leave my other rows alone, SO THAT the status line keeps working.

ACCEPTANCE CRITERIA

- [x] STL-4_AC-1 [conditional]: IF anything fails (bad stdin, unreadable store or config) THEN the system SHALL print nothing (with `--json`, `{}`), write the reason to stderr and exit 0

### STL-5: status JSON [MUST]

AS A status line author, I WANT a stable JSON object, SO THAT I can build my own row with `jq`.

ACCEPTANCE CRITERIA

- [x] STL-5_AC-1 [ubiquitous]: The system SHALL print one object with camelCase keys `session`, `idle`, `machine`, `state`, `visit`, `yielded`, `instance`, `suspended`, `parked`, every key present (`null` when not applicable) — D2-6
- [x] STL-5_AC-2 [ubiquitous]: `instance` (the held instance) and `suspended` (the detoured-from instance) SHALL each be `null` or `{machine, kind, id, ref, label, status}`, where `label` is the ref once set, else the id — D2-8

### STL-6: default row [MUST]

AS AN agent user, I WANT a readable coloured row with no setup, SO THAT one line in my script is enough.

ACCEPTANCE CRITERIA

- [x] STL-6_AC-1 [state]: WHILE the session is in a state machine the system SHALL print `smllm <machine> › <STATE>[ (visit n)] · <kind> <label>[ · yielded]`, `(visit n)` from the second visit — D2-7
- [x] STL-6_AC-2 [state]: WHILE the session is in idle the system SHALL print `smllm idle[ · n suspended][ · n parked]`, each part only when non-zero
- [x] STL-6_AC-3 [ubiquitous]: The row SHALL colour `smllm` dim, the machine cyan, the state bold, the instance magenta, notes dim and `yielded` yellow, using plain text separators only — D2-4

### STL-7: colour choice [MUST]

AS AN agent user, I WANT colour by default and a way to turn it off, SO THAT the row fits my terminal.

ACCEPTANCE CRITERIA

- [x] STL-7_AC-1 [complex]: WHEN the row is printed THEN the system SHALL colour it per `--color` (`auto` = the next rule); without `--color`, never IF `NO_COLOR` is set and non-empty, otherwise always (the harness captures stdout, so a TTY check would always say no)

### STL-8: core status [MUST]

AS A harness author, I WANT the status from the engine, SO THAT wasm and other hosts show the same fields.

ACCEPTANCE CRITERIA

- [x] STL-8_AC-1 [ubiquitous]: The core SHALL expose a read-only `Engine::status(host, key)` returning the STL-5 fields, and `smllm-wasm` SHALL expose it as `Engine.status(key)` returning JSON

### STL-9: setup skill [MUST]

AS AN agent user, I WANT to ask Claude to add smllm to my status line, SO THAT I do not edit scripts by hand.

ACCEPTANCE CRITERIA

- [x] STL-9_AC-1 [ubiquitous]: The system SHALL ship a skill `smllm-statusline` that adds the smllm row to the user's existing status line (or creates a minimal one), customises it on request against `--json`, verifies it with a sample status JSON, and removes it on request, each change shown and applied only with the user's consent
- [x] STL-9_AC-2 [event]: WHEN `harness install claude` runs THEN the system SHALL install the skill as part `statusline` (project `.claude/skills/`, user `~/.claude/skills/`), unless declined with `--without statusline`; the plugin SHALL ship it too — D2-10
- [x] STL-9_AC-3 [ubiquitous]: `harness install` SHALL NOT change the `statusLine` setting — D2-2

### STL-10: user doc [SHOULD]

AS AN agent user, I WANT one page on the status line, SO THAT I can set it up or customise it myself.

ACCEPTANCE CRITERIA

- [x] STL-10_AC-1 [ubiquitous]: `docs/statusline.md` SHALL cover setup (the skill, or the one line), the default row, the JSON fields, custom-row recipes and other harnesses; the README and the skill SHALL link to it — D2-11

### STL-11: setup hint [SHOULD]

AS AN agent user, I WANT to be told the row is not set up, SO THAT I know the skill exists.

ACCEPTANCE CRITERIA

- [x] STL-11_AC-1 [conditional]: IF `harness install|status claude` finds no `statusLine` setting, or one whose command (or the script it runs) does not call `smllm statusline` THEN the text report SHALL end with a note on stderr: ask Claude to add smllm to your status line

DEPENDS ON: HOST-6

## Constraints

- No new dependencies
- The JSON contract (STL-5) is snapshot-tested; fields are added freely, renamed or removed only in a minor release before 1.0 (NFR-6)

## Out of Scope

- A template language or style config for the row (D2-1)
- A `subagentStatusLine` row (D2-5)
- Nerd Font glyphs in the default row (D2-4)

## Change Log

- 0.1.0 (2026-09-25): Initial requirements from PLAN-002
