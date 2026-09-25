# Requirements Specification

## Introduction

Instances (INST): every piece of work a session does inside a state machine is an instance with a generated id, an optional set-once ref, a status and one holding session. Source: PLAN-001 §1, §4 (INST table), decisions D7, D13, D23.

## Glossary

- INSTANCE: One piece of work moving through a state machine; stored beside its config
- INSTANCE ID: The generated key `i-xxxxxx`, permanent
- REF: An optional external id (e.g. `GH-123`), unique per state machine, set once
- ID PARAM: The one param name the agent uses for an instance (the machine's `meta.instance.ref.param`)
- HOLDER: The session currently working on an instance
- STATUS: active, suspended, parked or completed
- VERSION: A counter bumped on every instance write, used to detect concurrent writers

## Stakeholders

- AGENT: Fires events and reads the instance shown in every header
- USER: Runs several harness sessions that may pick up the same work
- HOST: Stores instances and enforces write ordering

## Requirements

### INST-1: Always an instance [MUST]

AS AN agent, I WANT every state machine session to work on an instance, SO THAT state, visits and history always have an owner.

ACCEPTANCE CRITERIA

- [ ] INST-1_AC-1 [ubiquitous]: The system SHALL NOT place a session in a state machine state without an instance it holds — structural: a session is either idle or holds one instance

### INST-2: Generated instance id [MUST]

AS A host, I WANT a generated permanent id per instance, SO THAT state and history have a stable key even before a ref exists.

ACCEPTANCE CRITERIA

- [x] INST-2_AC-1 [event]: WHEN a new instance is created THEN the system SHALL give it a fresh generated id of the form `i-xxxxxx`, unique within its state machine, and SHALL never change it

### INST-3: Set-once ref [MUST]

AS AN agent, I WANT to name an instance by its external id, SO THAT I can refer to the work as the user does.

ACCEPTANCE CRITERIA

- [x] INST-3_AC-1 [complex]: WHERE a ref is given at `enter` or by a `setRef` action, the system SHALL set it only if the instance has no ref and no other instance of the same state machine has that ref or id; IF the ref is already set or taken THEN the system SHALL reject the call with an error and change nothing

### INST-4: One id param [MUST]

AS AN agent, I WANT one id param per state machine, SO THAT I do not track two ids.

ACCEPTANCE CRITERIA

- [ ] INST-4_AC-1 [ubiquitous]: The system SHALL show an instance by its ref once set, else by its generated id, and SHALL resolve either value given in the id param to the same instance

### INST-5: Instance status [MUST]

AS A user, I WANT each instance to have a status, SO THAT I can see what is in progress, set aside or done.

ACCEPTANCE CRITERIA

- [ ] INST-5_AC-1 [ubiquitous]: The system SHALL keep each instance in exactly one status: active (held by a session), suspended (detour), parked, or completed (final state reached, kept)

### INST-6: Single holder with takeover [MUST]

AS A user, I WANT a second session to be able to pick up work, SO THAT a stale conversation does not lock it.

ACCEPTANCE CRITERIA

- [x] INST-6_AC-1 [event]: WHEN `enter` targets an instance held by another session THEN the system SHALL make the calling session the only holder and SHALL report the previous holder and its last activity time in the header

### INST-7: Superseded session is told [MUST]

AS AN agent, I WANT to learn that my instance moved, SO THAT I stop working on it.

ACCEPTANCE CRITERIA

- [x] INST-7_AC-1 [event]: WHEN a session calls while its instance is held by another session, no longer active, or gone THEN the system SHALL return an error saying where it moved (e.g. "moved to session …") and SHALL put the session in idle

### INST-8: Versioned writes [MUST]

AS A user, I WANT concurrent sessions not to overwrite each other, SO THAT instance state stays consistent.

ACCEPTANCE CRITERIA

- [ ] INST-8_AC-1 [conditional]: IF an instance write does not carry the stored version plus one THEN the system SHALL refuse it, treat the caller as the loser, put its session in idle with an error, and write no history
- [x] INST-8_AC-2 [ubiquitous]: The file store SHALL compare the stored and new versions and write the instance while holding an OS file lock on the machine's state directory

DEPENDS ON: INST-7

### INST-9: Completed instances are kept [MUST]

AS A user, I WANT finished work kept, SO THAT I can review or reopen it.

ACCEPTANCE CRITERIA

- [ ] INST-9_AC-1 [ubiquitous]: The system SHALL NOT offer the agent any way to delete an instance

### INST-10: Reopen a completed instance [SHOULD]

AS AN agent, I WANT to reopen finished work at an entry point, SO THAT follow-up work keeps its history.

ACCEPTANCE CRITERIA

- [x] INST-10_AC-1 [complex]: WHEN `enter` targets a completed instance, IF no `state` is given THEN the system SHALL not move and SHALL list the entry points in the error; IF `state` is a `meta.entryPoint` state THEN the system SHALL make the instance active there and say it was reopened in the header

DEPENDS ON: INST-11

### INST-11: History carries across reopen [SHOULD]

AS A user, I WANT visit counts and history kept on reopen, SO THAT `visits` guards and audits still see the whole story.

ACCEPTANCE CRITERIA

- [x] INST-11_AC-1 [event]: WHEN an instance is reopened THEN the system SHALL keep its visit counts and history and continue counting from them

## Assumptions

- A host supplies randomness (`Ids`) for ids and storage (`Store`) for records (NFR-1)

## Constraints

- Instances live beside their config (STO-1); sessions are user-level

## Out of Scope

- Sharing instances between checkouts (O6: private in v1)
- Deleting instances

## Change Log

- 1.0.0 (2026-09-25): Initial requirements from PLAN-001 §4
