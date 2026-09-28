# Requirements Specification

## Introduction

Idle and the built-in events (IDLE). Idle is smllm's own state outside every state machine; its entry block is the idle list. The built-ins `enter`, `resume`, `pause`, `unmatched` and `yield` move instances in and out of state machines; `listPaused` lists paused instances. Source: PLAN-001 §1, §5, decisions D8–D12, D22.

## Glossary

- IDLE: The session's resting place when it holds no instance
- IDLE LIST: Idle's entry block: idle text, state machines (id, description, id param, initial state, entry points), the interrupted instance, the most recently updated paused instances (IDLE-7), and the idle events
- BUILT-IN EVENT: `enter` (idle), `resume` (idle when interrupted; a fallback state), `listPaused` (idle, when the list of paused instances was cut short), `pause`, `unmatched`, `yield` (every state)
- INTERRUPTED: An instance put aside by `unmatched` for a detour; `resume` returns to it
- PAUSED: An instance put aside by `pause` to continue later; `enter` with its ref continues it
- ENTRY POINT: A state with `meta.entryPoint: true`; may be entered directly from idle
- FALLBACK STATE: The one state with `meta.fallback: true`; `unmatched` goes there instead of idle
- DETOUR: `unmatched` interrupting an instance so the agent can handle something else, then `resume`
- JUMP: `enter` of an existing instance with an explicit entry point `state`

## Stakeholders

- AGENT: Chooses work from the idle list and uses the built-ins
- MACHINE AUTHOR: Overrides built-in guidance and may mark a fallback state

## Requirements

### IDLE-1: Fixed built-in names [MUST]

AS A machine author, I WANT to reword built-in guidance, SO THAT it fits my workflow, without changing how built-ins behave.

ACCEPTANCE CRITERIA

- [ ] IDLE-1_AC-1 [ubiquitous]: The system SHALL reserve the built-in event names, SHALL use a machine's `meta.events.<name>.description` as that built-in's guidance, and SHALL NOT let a machine change a built-in's params

### IDLE-2: Fallback state [SHOULD]

AS A machine author, I WANT `unmatched` to go to a state of my machine, SO THAT side requests are handled inside the workflow.

ACCEPTANCE CRITERIA

- [x] IDLE-2_AC-1 [complex]: WHERE a machine marks one state `meta.fallback: true`, WHEN `unmatched` fires in another state THEN the system SHALL enter the fallback state with the instance still active, remember the interrupted state, and offer `resume` there to return to it; otherwise `unmatched` SHALL interrupt the instance and put the session in idle

### IDLE-3: Missing saved state [MUST]

AS AN agent, I WANT a clear way out when my saved state was removed from the config, SO THAT the instance is not stuck.

ACCEPTANCE CRITERIA

- [x] IDLE-3_AC-1 [conditional]: IF the saved state of an instance no longer exists on `enter` or `resume` without `state` THEN the system SHALL not move and SHALL list all states; WHEN `enter` gives any existing `state` THEN the system SHALL enter it (repair)

### IDLE-4: Validate saved states [SHOULD]

AS A machine author, I WANT to know which saved instances my edit broke, SO THAT I can repair them.

ACCEPTANCE CRITERIA

- [ ] IDLE-4_AC-1 [event]: WHEN `smllm validate` runs THEN the system SHALL warn about each saved instance whose state no longer exists

### IDLE-5: Final states [MUST]

AS AN agent, I WANT reaching a final state to finish the work, SO THAT I return to idle.

ACCEPTANCE CRITERIA

- [x] IDLE-5_AC-1 [event]: WHEN an instance enters a `type: final` state THEN the system SHALL run its entry actions (showing its prompt), mark the instance completed, and put the session in idle

### IDLE-6: Recorded jump [SHOULD]

AS A user, I WANT jumps to be visible, SO THAT skipped steps are auditable.

ACCEPTANCE CRITERIA

- [x] IDLE-6_AC-1 [event]: WHEN `enter` jumps an existing instance to an entry point THEN the system SHALL show `Arrived by: enter (jump from <STATE>)` and record the move in history

DEPENDS ON: INST-4

### IDLE-7: Bounded idle list [MUST]

AS AN agent, I WANT the idle list to stay short however many instances are paused, SO THAT idle costs little context and time, and I can still see every paused instance when I need to.

> A realistic maximum is ~100 paused instances; listing them all on every return to idle wastes context (PLAN-008).

ACCEPTANCE CRITERIA

- [ ] IDLE-7_AC-1 [ubiquitous]: The idle list SHALL show at most 10 paused instances, most recently updated first (ties by machine id, then label), followed by the number of paused instances not shown
- [ ] IDLE-7_AC-2 [event]: WHEN the idle list was cut short THEN the system SHALL offer the built-in `listPaused`; WHEN `listPaused` fires THEN the system SHALL reply with every paused instance, in the same order, and the idle events, changing no session, instance or history

## Assumptions

- Only one instance per session may be interrupted at a time

## Constraints

- `enter` params: `stateMachine` (required), the machine's id param, `state`; a new instance starts at `state` (an entry point) or `initial`

## Out of Scope

- Nested or parallel machines (D1)
- An agent `finish`/abandon built-in (D10)

## Change Log

- 1.0.0 (2026-09-25): Initial requirements from PLAN-001 §5
- 1.1.0 (2026-09-28): Terms pause and interrupted (were park and suspend); IDLE-7 bounded idle list and `listPaused` (PLAN-008)
