# Requirements Specification

## Introduction

Idle and the built-in events (IDLE). Idle is smllm's own state outside every state machine; its entry block is the idle list. The built-ins `enter`, `resume`, `park`, `unmatched` and `yield` move instances in and out of state machines. Source: PLAN-001 §1, §5, decisions D8–D12, D22.

## Glossary

- IDLE: The session's resting place when it holds no instance
- IDLE LIST: Idle's entry block: idle text, state machines (id, description, id param, initial state, entry points), the suspended instance, parked instances, and the idle events
- BUILT-IN EVENT: `enter` (idle), `resume` (idle when suspended; a fallback state), `park`, `unmatched`, `yield` (every state)
- ENTRY POINT: A state with `meta.entryPoint: true`; may be entered directly from idle
- FALLBACK STATE: The one state with `meta.fallback: true`; `unmatched` goes there instead of idle
- DETOUR: `unmatched` suspending an instance so the agent can handle something else, then `resume`
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

- [x] IDLE-2_AC-1 [complex]: WHERE a machine marks one state `meta.fallback: true`, WHEN `unmatched` fires in another state THEN the system SHALL enter the fallback state with the instance still active, remember the interrupted state, and offer `resume` there to return to it; otherwise `unmatched` SHALL suspend the instance and put the session in idle

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

## Assumptions

- Only one instance per session may be suspended at a time

## Constraints

- `enter` params: `stateMachine` (required), the machine's id param, `state`; a new instance starts at `state` (an entry point) or `initial`

## Out of Scope

- Nested or parallel machines (D1)
- An agent `finish`/abandon built-in (D10)

## Change Log

- 1.0.0 (2026-09-25): Initial requirements from PLAN-001 §5
