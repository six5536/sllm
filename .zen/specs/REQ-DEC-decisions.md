# Requirements Specification

## Introduction

Branching (DEC): how a state machine, not the agent, decides where to go — guarded transitions, eventless `always` states, the `command` and `visits` guards, and the command runner shared with actions. Source: PLAN-001 §6 (DEC rows), NFR-5, decisions D16, D17.

## Glossary

- GUARD: A condition on a transition, `{type, params}`
- GUARDED ARRAY: An event's ordered list of candidate transitions
- ALWAYS STATE: A state with eventless `always` transitions; left immediately
- COMMAND: A `command` guard or action: `run` as a string (shell) or an array (exec)
- TRACE: The event, the transition taken, guard results and output tails

## Stakeholders

- MACHINE AUTHOR: Writes guards and commands
- AGENT: Sees guard results in the next header
- USER: Trusts machine files like a Makefile (NFR-5)

## Requirements

### DEC-1: First matching transition [MUST]

AS A machine author, I WANT guarded transitions tried in order, SO THAT branching is predictable (XState).

ACCEPTANCE CRITERIA

- [ ] DEC-1_AC-1 [ubiquitous]: The system SHALL evaluate a guarded array in order and take the first transition whose guard passes or that has no guard

### DEC-2: Always states [MUST]

AS A machine author, I WANT eventless decision states, SO THAT the machine branches without an agent turn.

ACCEPTANCE CRITERIA

- [ ] DEC-2_AC-1 [ubiquitous]: The system SHALL leave an `always` state immediately by its first matching `always` transition, SHALL never rest a session in one, and SHALL stop a chain after a fixed length cap, reporting it
- [ ] DEC-2_AC-2 [event]: WHEN a state machine is validated THEN the validator SHALL reject any cycle made only of `always` transitions

### DEC-3: Guard types [MUST]

AS A machine author, I WANT `command` and `visits` guards, SO THAT I can branch on tool results and loop counts.

ACCEPTANCE CRITERIA

- [ ] DEC-3_AC-1 [ubiquitous]: The system SHALL support the guard types `command` (evaluated by the host) and `visits` (evaluated by the engine: true when the state's visit count, including the current entry, is at least `atLeast`)

### DEC-4: Command forms [MUST]

AS A machine author, I WANT shell and exec forms, SO THAT I can choose convenience or no shell.

ACCEPTANCE CRITERIA

- [ ] DEC-4_AC-1 [ubiquitous]: The system SHALL run a string `run` through the system shell (`sh -c`, or `cmd /C` on Windows) and an array `run` directly as a program and arguments

### DEC-5: Command results [MUST]

AS A machine author, I WANT exit codes to decide guards, SO THAT any tool can be a guard.

ACCEPTANCE CRITERIA

- [ ] DEC-5_AC-1 [ubiquitous]: The system SHALL treat exit 0 as true and a non-zero exit, timeout or spawn failure as false with a reason; `timeoutSecs` SHALL default to 60; the output tail SHALL be captured

### DEC-6: Command environment [MUST]

AS A user, I WANT LLM input to reach commands only as environment, SO THAT it cannot inject shell code through templates.

ACCEPTANCE CRITERIA

- [ ] DEC-6_AC-1 [ubiquitous]: The system SHALL pass `SMLLM_SESSION`, `SMLLM_MACHINE`, `SMLLM_STATE`, `SMLLM_EVENT`, `SMLLM_INSTANCE`, `SMLLM_REF` and `SMLLM_PARAM_<NAME>` (SCREAMING_SNAKE of the param name) as environment and SHALL NOT interpolate them into the command

### DEC-7: Working directory [MUST]

AS A machine author, I WANT commands to run in the agent's project, SO THAT relative paths work.

ACCEPTANCE CRITERIA

- [ ] DEC-7_AC-1 [ubiquitous]: The system SHALL run commands in the session's working directory recorded at bind, unless `params.cwd` is given, which SHALL be resolved relative to the YAML file

### DEC-8: Trace [SHOULD]

AS AN agent, I WANT to see why the machine chose a state, SO THAT I can act on failures.

ACCEPTANCE CRITERIA

- [ ] DEC-8_AC-1 [event]: WHEN a transition is taken THEN the system SHALL record the event, the transition, each guard result and output tail in history and show them in the next header

### DEC-9: No shell for arrays [MUST]

AS A machine author, I WANT array commands to be literal, SO THAT arguments are never re-parsed.

ACCEPTANCE CRITERIA

- [ ] DEC-9_AC-1 [ubiquitous]: The system SHALL NOT expand `$SMLLM_*` in array-form commands; they reach the program only as environment

## Assumptions

- Machine files are trusted like a Makefile (NFR-5)

## Constraints

- The engine core evaluates `visits` itself; `command` runs in the host (NFR-1, NFR-9)

## Out of Scope

- Guard types other than `command` and `visits` in v1

## Change Log

- 1.0.0 (2026-09-25): Initial requirements from PLAN-001 §6
