# Requirements Specification

## Introduction

How agent harnesses reach the smllm engine: the core protocol, session keys, the MCP and CLI surfaces, and the Claude Code integration (hooks, MCP server, instructions block, plugin). Source: PLAN-001 §8 (HOST-1..12 and the parts table), decisions D3, D5, D15, D29, D31, D34. The turn-loop behaviour behind the hooks is in the TURN requirements.

## Glossary

- HARNESS: the agent runtime smllm plugs into (Claude Code first)
- HARNESS SESSION ID: the harness's own conversation id (Claude Code `session_id`)
- SESSION KEY: smllm's key for a session, e.g. `sm-k7f3q2`
- PART: one piece of a harness integration: `instructions`, `mcp`, `hooks`, `permissions`, `statusline` (the setup skill, STL-9)
- ENTRY BLOCK / EVENTS LIST: the `<smllm>` texts defined in TURN-1, TURN-2

## Stakeholders

- AGENT USER: installs smllm into a harness and works through sessions
- AGENT: the LLM that receives entry blocks and calls the `smllm` tool
- HARNESS MAINTAINER: adds support for another harness

## Requirements

### HOST-1: Core protocol [MUST]

AS A harness maintainer, I WANT one small protocol in the core, SO THAT every surface behaves the same.

ACCEPTANCE CRITERIA

- [ ] HOST-1_AC-1 [ubiquitous]: The core SHALL expose `bind(harness, hostSessionId?)`, `view(key)`, `menu(key)` and `fire(key, event, params)`, and every surface SHALL reach the engine only through them — the hook-only `stop` and `prompt_submitted` sit beside them (TURN-4, TURN-8)

### HOST-2: Session key [MUST]

AS AN agent, I WANT a short session key in every header, SO THAT I can pass it back on each call.

ACCEPTANCE CRITERIA

- [ ] HOST-2_AC-1 [event]: WHEN a session is created THEN the system SHALL give it a short random key unique among stored sessions, and SHALL show it in every header (TURN-1)

### HOST-3: Unknown or missing key [MUST]

AS AN agent, I WANT a clear error for a bad key, SO THAT I can recover; AS A shell-only harness, I WANT to start a session with `enter`, SO THAT hooks are optional.

ACCEPTANCE CRITERIA

- [x] HOST-3_AC-1 [complex]: IF a call names an unknown key or omits the key THEN the system SHALL reject it with an error and a hint; WHERE the key is omitted and the event is `enter` the system SHALL instead bind a new session

### HOST-4: New harness session, new key [MUST]

AS AN agent user, I WANT `/clear` or a fork to start fresh, SO THAT old context does not leak.

ACCEPTANCE CRITERIA

- [x] HOST-4_AC-1 [event]: WHEN a harness session id with no binding starts THEN the system SHALL create a new session key in idle, and held instances SHALL move to it only through `enter` (INST-6); a known id SHALL get its existing key back

### HOST-5: Two surfaces, one core [MUST]

AS A harness maintainer, I WANT the tool as an MCP server and as a CLI command, SO THAT MCP and shell-only harnesses are both served.

ACCEPTANCE CRITERIA

- [ ] HOST-5_AC-1 [ubiquitous]: The system SHALL offer the tool as the MCP tool of `smllm mcp` and as `smllm fire`, both calling the same core engine

### HOST-6: Claude Code install and plugin [MUST]

AS AN agent user, I WANT a one-command Claude Code install, SO THAT hooks, tool and rules are wired correctly.

ACCEPTANCE CRITERIA

- [x] HOST-6_AC-1 [event]: WHEN the user runs `smllm harness install claude [--scope project|user]` THEN the system SHALL install the `instructions`, `mcp`, `hooks` and `permissions` parts at the locations of the PLAN-001 §8 parts table, as sokf does
- [ ] HOST-6_AC-2 [ubiquitous]: The project SHALL ship a Claude Code plugin with the three hooks and the `smllm` MCP server, installable from the repository's plugin marketplace
- [ ] HOST-6_AC-3 [conditional]: IF the scope is `user` THEN the system SHALL register the MCP server only through `claude mcp add-json --scope user` / `claude mcp remove`, and SHALL only read `~/.claude.json`, never write it

### HOST-7: Hook answers never wedge the agent [MUST]

AS AN agent user, I WANT a failing hook to fail softly, SO THAT the agent is never stuck.

ACCEPTANCE CRITERIA

- [x] HOST-7_AC-1 [complex]: The stop hook SHALL answer `{}` or `{"decision":"block","reason":"<smllm>…</smllm>"}`; WHEN any hook fails THEN it SHALL exit 1 with the error on stderr and nothing on stdout

### HOST-8: No config, silent hooks [MUST]

AS AN agent user, I WANT hooks installed at user scope to stay quiet outside smllm projects, SO THAT other work is unaffected.

ACCEPTANCE CRITERIA

- [x] HOST-8_AC-1 [conditional]: IF no config is found (and no session is bound) THEN every hook SHALL answer `{}`

### HOST-9: Hook behaviour spike [SHOULD]

AS A maintainer, I WANT Claude Code's hook behaviour confirmed hands-on, SO THAT the design rests on observed facts.

ACCEPTANCE CRITERIA

- [x] HOST-9_AC-1 [ubiquitous]: The project SHALL record whether SessionStart and UserPromptSubmit `additionalContext` reach the model and whether `--resume` keeps `session_id` — done 2026-09-25 (Claude Code 2.1.282): all confirmed; results in PLAN-001 §15 [untestable]

### HOST-10: Instructions target file [MUST]

AS AN agent user, I WANT the orientation block in the file my harness reads, SO THAT I keep one instructions file.

ACCEPTANCE CRITERIA

- [ ] HOST-10_AC-1 [ubiquitous]: The system SHALL choose the target as sokf does: no `AGENTS.md` → `CLAUDE.md` (created if absent); only `AGENTS.md` → it; both with an `@AGENTS.md` line in `CLAUDE.md` → `AGENTS.md`; both without the import → `CLAUDE.md`

### HOST-11: Marker block [MUST]

AS AN agent user, I WANT smllm's block kept apart from my text, SO THAT reinstalling never touches my content.

ACCEPTANCE CRITERIA

- [x] HOST-11_AC-1 [ubiquitous]: The system SHALL keep its block between `<!-- smllm:harness -->` and `<!-- /smllm:harness -->`: replace between the first markers, else append after one blank line; an empty or absent file becomes the block; keep line endings; write only on change; a blank line after the opening marker; compare by words; an edited block is replaced only with `--force`

### HOST-12: Tool description carries the rules [MUST]

AS AN agent, I WANT the full rules in the tool description, SO THAT every MCP harness gets them without an instructions file.

ACCEPTANCE CRITERIA

- [x] HOST-12_AC-1 [ubiquitous]: The MCP tool description SHALL carry the full agent rules (TURN-9), the same for every MCP harness, as a stable contract covered by a snapshot test — the current test asserts content, not a snapshot

## Assumptions

- Claude Code's hook JSON (`session_id`, `cwd`, `stop_hook_active`, `hookSpecificOutput.additionalContext`, `decision: block`) behaves as documented; HOST-9 confirms it

## Constraints

- Only the main agent calls `smllm` (D15); the session key is never passed to subagents
- Parts table (PLAN-001 §8): `instructions` in `AGENTS.md`/`CLAUDE.md` beside `.smllm/` or in `~/.claude/`; `mcp` in `.mcp.json` or via `claude mcp`; `hooks` and `permissions` in `.claude/settings.json` or `~/.claude/settings.json`

## Out of Scope

- Harnesses other than Claude Code (O3)
- A headless `smllm run` driver

## Change Log

- 0.1.0 (2026-09-25): Initial requirements from PLAN-001 §8
