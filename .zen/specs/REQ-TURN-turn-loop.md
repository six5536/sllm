# Requirements Specification

## Introduction

The agent interaction loop (TURN): what the agent sees on entering a state, what the stop hook shows, how calls are answered, and the stable `<smllm>` text contract. The exact text layout is in DESIGN-TURN-turn-loop.md (Data Models). Source: PLAN-001 §7, decisions D2, D3, D11, D12, D15, D18, D21, D33.

## Glossary

- ENTRY BLOCK: Text returned on arriving in a state: header, arrival lines, `<instructions>`
- EVENTS LIST: The menu: header, the call line, `<events>`
- ERROR BLOCK: Header, `error:` line, events list; returned for an invalid call
- IDLE LIST: Idle's entry block (REQ-IDLE)
- HEADER: The first line of every block: session key, machine › state, visit, kind + id/ref
- YIELDED FLAG: Set by `yield`, cleared by a user prompt or any transition
- STOP HOOK: The harness hook run when the agent tries to end its turn

## Stakeholders

- AGENT: Reads smllm text and calls the `smllm` tool
- HARNESS: Runs the session-start, user-prompt-submit and stop hooks (REQ-HOST)
- MACHINE AUTHOR: Writes the text shown inside `<instructions>`

## Requirements

### TURN-1: Header [MUST]

AS AN agent, I WANT a fixed header, SO THAT I always know where I am.

ACCEPTANCE CRITERIA

- [x] TURN-1_AC-1 [ubiquitous]: The system SHALL start every block with `session <key> · <machine> › <STATE>`, then ` (visit n)` from the second visit, then ` · <kind> <id or ref>`; followed by the arriving event, params, notes, transition trace and failed actions, each on its own line

### TURN-2: Events list [MUST]

AS AN agent, I WANT each offered event with its params, SO THAT I can fire one correctly.

ACCEPTANCE CRITERIA

- [x] TURN-2_AC-1 [ubiquitous]: The system SHALL list the state's own events first, then the built-ins, each with its guidance, and under each its params with required/optional, enum values or pattern, and prompt

### TURN-3: Call results [MUST]

AS AN agent, I WANT invalid calls rejected with the menu, SO THAT I can correct them without side effects.

ACCEPTANCE CRITERIA

- [x] TURN-3_AC-1 [complex]: WHEN a call is valid THEN the system SHALL return the next entry block; IF it is invalid (event not offered, unknown/missing/invalid param, no transition matched, ref already set) THEN the system SHALL return an error block with the events list and change nothing

### TURN-4: Stop hook blocks [MUST]

AS A user, I WANT the agent held to the machine, SO THAT it does not stop mid-state without choosing an event.

ACCEPTANCE CRITERIA

- [x] TURN-4_AC-1 [state]: WHILE the session is in a machine state and has not yielded since the last user prompt, the stop hook SHALL block with the events list

### TURN-5: Idle may stop [MUST]

AS AN agent, I WANT to stop freely in idle, SO THAT idle never traps me.

ACCEPTANCE CRITERIA

- [x] TURN-5_AC-1 [state]: WHILE the session is in idle (or unknown), the stop hook SHALL allow the stop

### TURN-6: Runaway guard [MUST]

AS A user, I WANT a stuck agent released, SO THAT a stop hook loop cannot run forever.

ACCEPTANCE CRITERIA

- [x] TURN-6_AC-1 [conditional]: IF the harness reports it is already continuing because of a stop hook (Claude Code `stop_hook_active`) and the agent still fired no event THEN the stop hook SHALL allow the stop and write the events list to stderr — sokf's same-report hash is the fallback for harnesses without the flag

### TURN-7: Idle list on entry only [MUST]

AS AN agent, I WANT the idle list only when I arrive in idle, SO THAT prompts are not flooded.

ACCEPTANCE CRITERIA

- [x] TURN-7_AC-1 [event]: WHEN the session enters idle (session start/resume/compact, `park`, `unmatched`, final state, moved) THEN the system SHALL return the idle list, and SHALL NOT inject it on user prompts

### TURN-8: User prompt [MUST]

AS AN agent, I WANT each user prompt to start a fresh turn, SO THAT a previous `yield` does not let me skip the machine.

ACCEPTANCE CRITERIA

- [x] TURN-8_AC-1 [event]: WHEN a user prompt arrives THEN the system SHALL clear the yielded flag and inject nothing

### TURN-9: Agent rules [MUST]

AS AN agent, I WANT the loop's rules stated once, SO THAT I call the tool correctly.

ACCEPTANCE CRITERIA

- [ ] TURN-9_AC-1 [ubiquitous]: The system SHALL provide rules text telling the agent to fire `yield` before stopping to ask, to call with no event to re-orient, and that only the main agent calls `smllm` and never passes the key to subagents

### TURN-10: Params not persisted [MUST]

AS A user, I WANT params kept only where needed, SO THAT instance state stays small (D18).

ACCEPTANCE CRITERIA

- [ ] TURN-10_AC-1 [ubiquitous]: The system SHALL NOT store params on the instance; only the next header and the history entry SHALL carry them

### TURN-11: Stable text contract [MUST]

AS A harness author, I WANT the text format stable, SO THAT agents and tools can rely on it.

ACCEPTANCE CRITERIA

- [x] TURN-11_AC-1 [ubiquitous]: The agent text format SHALL be covered by snapshot tests

### TURN-12: Fenced output [MUST]

AS AN agent, I WANT smllm's text fenced apart from author text, SO THAT I can tell instructions from the menu.

ACCEPTANCE CRITERIA

- [ ] TURN-12_AC-1 [ubiquitous]: The system SHALL wrap each reply in exactly one `<smllm>…</smllm>` fence, with author text in `<instructions>` and the menu in `<events>`
- [ ] TURN-12_AC-2 [event]: WHEN a state machine is validated THEN the validator SHALL warn about author text containing `</smllm>`, `</instructions>` or `</events>`

## Assumptions

- The harness offers a stop hook that can block with a reason (Claude Code first)

## Constraints

- Everything smllm emits to the agent goes through one renderer (TURN-Render)

## Out of Scope

- Harness installation and hook JSON (REQ-HOST)

## Change Log

- 1.0.0 (2026-09-25): Initial requirements from PLAN-001 §7
