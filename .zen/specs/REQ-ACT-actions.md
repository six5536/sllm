# Requirements Specification

## Introduction

Actions (ACT): what runs when a transition is taken — `exit`, transition `actions` and `entry` lists holding `prompt`, `command` and `setRef` actions — in XState/SCXML order, without ever blocking a transition. Source: PLAN-001 §6 (ACT rows), NFR-9, decision D28.

## Glossary

- ACTION LIST: A state's `entry` or `exit`, a transition's `actions`, or a `meta.sharedActions` entry
- PROMPT ACTION: Text for the agent, gathered into `<instructions>`
- COMMAND ACTION: A command run by the host (REQ-DEC)
- EXTERNAL TRANSITION: One with a target other than the source, or a self target with `reenter: true`

## Stakeholders

- MACHINE AUTHOR: Writes action lists
- AGENT: Reads the gathered instructions and failure reports
- HOST: Runs action kinds it supports

## Requirements

### ACT-1: Execution order [MUST]

AS A machine author, I WANT XState's action order, SO THAT machines behave as in Stately's tools.

ACCEPTANCE CRITERIA

- [ ] ACT-1_AC-1 [ubiquitous]: The system SHALL run, after guards pick the transition, the source `exit`, then the transition `actions`, then the target `entry`, then repeat for `always` states; targetless and non-`reenter` self transitions SHALL run only their `actions`

### ACT-2: Commands and prompts [MUST]

AS AN agent, I WANT all instruction text in one block, SO THAT I read it once.

ACCEPTANCE CRITERIA

- [ ] ACT-2_AC-1 [ubiquitous]: The system SHALL run command actions in order as they are reached and SHALL concatenate prompt actions in the same order into one `<instructions>` block

### ACT-3: Actions never block [MUST]

AS AN agent, I WANT a failing command not to strand me, SO THAT the machine still moves and I learn what failed.

ACCEPTANCE CRITERIA

- [x] ACT-3_AC-1 [conditional]: IF a command action fails or times out THEN the system SHALL still complete the transition, skip the rest of that list's commands, and report the failure in the next header

### ACT-4: Shared command runner [MUST]

AS A machine author, I WANT actions to behave like guards, SO THAT I learn one command model.

ACCEPTANCE CRITERIA

- [ ] ACT-4_AC-1 [ubiquitous]: The system SHALL run command actions with the guard runner, forms, timeouts and cwd (DEC-4, DEC-5, DEC-7) and the DEC-6 environment plus `SMLLM_FROM` and `SMLLM_TO`

### ACT-5: Built-in actions [MUST]

AS A machine author, I WANT built-ins to run the lists a state change implies, SO THAT setup and teardown stay consistent.

ACCEPTANCE CRITERIA

- [ ] ACT-5_AC-1 [ubiquitous]: The system SHALL run the current state's `exit` on `park` and `unmatched`, the target's `entry` on `enter` and `resume`, and no actions on `yield`

### ACT-6: Host action kinds [MUST]

AS A host, I WANT to declare which action kinds I run, SO THAT unsupported ones are reported at load, not at run time.

ACCEPTANCE CRITERIA

- [ ] ACT-6_AC-1 [event]: WHEN a machine is loaded THEN the system SHALL report every action kind the host's `Action` implementation does not support as a finding

## Assumptions

- `setRef` is checked before a transition is taken (INST-3)

## Constraints

- Hosts supply action kinds through the core's `Action` trait (NFR-1)

## Out of Scope

- XState `assign` / `context` actions (CFG-2)

## Change Log

- 1.0.0 (2026-09-25): Initial requirements from PLAN-001 §6
