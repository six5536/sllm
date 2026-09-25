# Requirements Specification

## Introduction

The engine (ENG): which events are offered, how firing one moves an instance, how visits are counted, where instruction text comes from, and the read-only view. Source: PLAN-001 §1, §6 (ENG rows), NFR-1, NFR-3, decisions D2, D3, D33.

## Glossary

- OFFERED EVENTS: The events a call may fire in the current state
- FIRE: A call with an event: validate, transition, save, reply
- VISIT: One entry of a state by an instance
- VIEW: A call with no event; read-only
- ENTRY BLOCK: The `<smllm>` text returned on arriving in a state (REQ-TURN)

## Stakeholders

- AGENT: Fires events and reads entry blocks
- HOST: Supplies storage, guards, actions, files, time and randomness
- MACHINE AUTHOR: Writes instructions files and state machines

## Requirements

### ENG-1: Offered events [MUST]

AS AN agent, I WANT a fixed set of events per state, SO THAT I know what I may fire.

ACCEPTANCE CRITERIA

- [ ] ENG-1_AC-1 [ubiquitous]: The system SHALL offer in a machine state its `on` events followed by the built-ins `yield`, `park` and `unmatched` (plus `resume` in a fallback state, IDLE-2), and SHALL offer no events in a final state

### ENG-2: Fire [MUST]

AS AN agent, I WANT one call to validate and move the instance, SO THAT the machine, not I, decides the next state.

ACCEPTANCE CRITERIA

- [ ] ENG-2_AC-1 [event]: WHEN an event is fired THEN the system SHALL check it is offered and its params are valid, pick the transition by guards, run exit, transition actions and entry (ACT-1), leave any `always` states, save, and return the entry block of the state where the instance rests

DEPENDS ON: DEC-1, DEC-2, ACT-1

### ENG-3: Visit counting [MUST]

AS A machine author, I WANT visits counted per state, SO THAT `visits` guards and headers can use them.

ACCEPTANCE CRITERIA

- [ ] ENG-3_AC-1 [ubiquitous]: The system SHALL add one visit per instance and state on every state entry, including `reenter: true` self transitions and `resume`, and SHALL NOT count targetless or non-reentering self transitions

### ENG-4: Instructions read at request time [SHOULD]

AS A machine author, I WANT prompt file edits to show on the next call, SO THAT I can iterate quickly.

ACCEPTANCE CRITERIA

- [ ] ENG-4_AC-1 [ubiquitous]: The system SHALL read prompt files through the host each time they are shown; YAML edits take effect only when the config is loaded again

### ENG-5: Read-only view [MUST]

AS AN agent, I WANT to ask "where am I", SO THAT I can re-orient without side effects.

ACCEPTANCE CRITERIA

- [x] ENG-5_AC-1 [event]: WHEN a call has no event THEN the system SHALL return the current entry block and events list and SHALL make no transition, count no visit, write no history, run no command, and not count as a yield

## Assumptions

- The model has been validated by `smllm-format` before the engine runs it (REQ-CFG)

## Constraints

- NFR-1: the engine has no IO, time or randomness; hosts supply them
- NFR-3: same state, input and guard results give the same output

## Out of Scope

- Parsing or validating YAML (REQ-CFG)
- Persistence formats (REQ-STO)

## Change Log

- 1.0.0 (2026-09-25): Initial requirements from PLAN-001 §6
