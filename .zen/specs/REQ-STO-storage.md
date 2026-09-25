# Requirements Specification

## Introduction

Where smllm keeps sessions, harness bindings, instances and history, and how writes stay safe under concurrent sessions. Source: PLAN-001 §8 (STO-1..3), decisions D14 and O6. Instance semantics (versions, takeover) are in the INST requirements.

## Glossary

- USER STATE DIR: `$XDG_STATE_HOME/smllm` (default `~/.local/state/smllm`; platform equivalent elsewhere)
- CONFIG DIR: the directory holding a `config.toml` that lists state machines
- BINDING: a harness session id mapped to an smllm session key

## Stakeholders

- AGENT USER: runs sessions in one or more projects
- CONCURRENT SESSION: a second harness session touching the same instance

## Requirements

### STO-1: Sessions are user-level, instances live beside their config [MUST]

AS AN agent user, I WANT sessions kept per user and instances kept with the project's config, SO THAT a session can span projects while work state stays with the work.

> D14; O6 (instances private in v1).

ACCEPTANCE CRITERIA

- [ ] STO-1_AC-1 [ubiquitous]: The system SHALL store sessions and bindings under the user state dir, and each state machine's instances and history in a `state/` directory beside the config that lists that machine.
- [ ] STO-1_AC-2 [event]: WHEN the system first writes an instance under a `state/` directory THEN it SHALL create a `.gitignore` there that ignores the directory's contents.

### STO-2: A session is bound to its configs at creation [MUST]

AS AN agent user, I WANT a session to keep using the configs it started with, SO THAT later calls from another working directory act on the same machines.

ACCEPTANCE CRITERIA

- [ ] STO-2_AC-1 [event]: WHEN a session is created THEN the system SHALL record on it the config files found from the harness working directory (CLI-3_AC-2), and every later call for that session SHALL load exactly those recorded configs.

DEPENDS ON: CLI-3

### STO-3: Atomic, locked, versioned writes [MUST]

AS A concurrent session, I WANT writes that never corrupt or silently overwrite each other, SO THAT two sessions cannot both move one instance.

ACCEPTANCE CRITERIA

- [x] STO-3_AC-1 [ubiquitous]: The system SHALL write every stored file (session, binding, instance, config edit) through a temporary file and a rename, so no reader sees a partial file.
- [ ] STO-3_AC-2 [ubiquitous]: The system SHALL write an instance only while holding an exclusive file lock for its state machine and only when the new version is the stored version plus one; otherwise it SHALL reject the write as a conflict (INST-8).

DEPENDS ON: INST-8

## Assumptions

- The file system supports atomic rename within a directory and advisory whole-file locks (std `File::lock`)

## Constraints

- File names derived from harness session ids and instance ids are sanitised to `[A-Za-z0-9_-]`

## Out of Scope

- Sharing instances through git (O6)
- Garbage collection of old sessions and bindings

## Change Log

- 0.1.0 (2026-09-25): Initial requirements from PLAN-001 §8
