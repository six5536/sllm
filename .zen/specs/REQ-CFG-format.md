# Requirements Specification

## Introduction

The smllm state machine file format (CFG): a strict subset of an XState v5 machine config written in YAML, with smllm-only data under XState's `meta`. Covers what a file may contain, how it is checked, and how problems are reported. Source: PLAN-001 §3 (CFG table), decisions D19, D20, D26, D32.

## Glossary

- STATE MACHINE: One `<id>.smllm.yaml` file; a flat set of states and the events that move between them
- STATE: A named node of a state machine; atomic or `type: final`
- EVENT: A named trigger that the agent fires (or that a built-in provides) to leave a state
- PARAM: A string value an event carries, declared under `meta.events.<name>.params`
- INSTANCE: One piece of work moving through a state machine
- REF: An instance's external id (e.g. an issue id), set once through the ref param
- FINDING: One reported problem in a file: file, line, YAML path, level (error, warning, info), message, fix hint and rule id
- BUILT-IN EVENT: An event smllm provides in every machine (e.g. `park`, `unmatched`, `yield`); see REQ-IDLE

## Stakeholders

- MACHINE AUTHOR: Writes state machine files and `config.toml`
- HOST: Loads validated machines to drive an agent (CLI app, `smllm-wasm`)
- TOOLING: Editors and Stately's visualiser that read the file or its JSON Schema

## Requirements

### CFG-1: XState v5 YAML file [MUST]

AS A machine author, I WANT to write a state machine as an XState v5 config in YAML, SO THAT existing XState tools can open it.

ACCEPTANCE CRITERIA

- [x] CFG-1_AC-1 [ubiquitous]: The system SHALL accept a state machine file only if it is a YAML XState v5 machine config with camelCase keys and `meta.smllm: 1`, and SHALL reject unknown keys with an error finding

### CFG-2: Supported XState subset [MUST]

AS A machine author, I WANT to know which XState features smllm supports, SO THAT I do not rely on ones it ignores.

> Nested, parallel and delayed behaviour is deferred past v1.

ACCEPTANCE CRITERIA

- [x] CFG-2_AC-1 [conditional]: IF a file uses an XState feature outside the subset (`id`, `initial`, `description`, `meta`, `states`, `on`, `always`, `entry`, `exit`, `actions`, `guard`, `target`, `reenter`, `type: final`) — such as nested `states`, `type: parallel`/`history`, `after`, `invoke`, `context`/`assign`, `output`, `tags` — THEN the system SHALL report an error finding with a hint that the feature is not in smllm v1

DEPENDS ON: CFG-1

### CFG-3: XState transition semantics [MUST]

AS A machine author, I WANT transitions to behave as in XState, SO THAT the file means the same thing in both.

ACCEPTANCE CRITERIA

- [ ] CFG-3_AC-1 [ubiquitous]: The system SHALL select the first transition whose guard passes in a guarded transition array, take `always` transitions without an event, treat a targetless transition as internal, and re-enter the current state on a self-target only when `reenter: true`

### CFG-4: Actions [MUST]

AS A machine author, I WANT `{type, params}` actions, SO THAT states can prompt the agent, run commands and set the ref.

ACCEPTANCE CRITERIA

- [ ] CFG-4_AC-1 [ubiquitous]: The system SHALL support the actions `prompt` (exactly one of `text`, `file`), `command` (`run`, `timeoutSecs`, `cwd`) and `setRef`, and SHALL accept a bare string as shorthand for a param-less action (`setRef`) — a state with no `prompt` entry action gets the implied prompt file `enter-<STATE>.md`

### CFG-5: Guards [MUST]

AS A machine author, I WANT `{type, params}` guards, SO THAT the machine can decide between transitions.

ACCEPTANCE CRITERIA

- [ ] CFG-5_AC-1 [ubiquitous]: The system SHALL support the guards `command` (`run`, `timeoutSecs`, `cwd`; exit 0 passes) and `visits` (`state`, `atLeast`; counts entries so far, including the current one)

### CFG-6: Instance vocabulary [MUST]

AS A machine author, I WANT to name an instance and its ref, SO THAT generated text uses my domain's words.

ACCEPTANCE CRITERIA

- [ ] CFG-6_AC-1 [ubiquitous]: The system SHALL read `meta.instance` with `noun` and `ref` (`param`, `description`, `pattern`), defaulting the noun to `instance` and the ref param to `ref`

### CFG-7: Event declarations [MUST]

AS A machine author, I WANT to declare each event type once, SO THAT its guidance and params are defined in one place.

ACCEPTANCE CRITERIA

- [x] CFG-7_AC-1 [ubiquitous]: The system SHALL read `meta.events.<name>` with `description` (default guidance) and `params` as a JSON Schema subset: `type: object`, `properties` of `type: string` with `description`, `enum`, `pattern`, and `required`

### CFG-8: Per-state overrides [SHOULD]

AS A machine author, I WANT to reword guidance and param prompts per state, SO THAT the agent gets context-specific help.

ACCEPTANCE CRITERIA

- [x] CFG-8_AC-1 [ubiquitous]: The system SHALL take per-state event guidance from a transition's `description` and per-state param prompts from the state's `meta.paramDescriptions`, which SHALL only reword declared params and never define new ones

DEPENDS ON: CFG-7

### CFG-9: Event usage rules [MUST]

AS A machine author, I WANT undeclared events to work and unused declarations to be flagged, SO THAT small machines stay short and mistakes are visible.

ACCEPTANCE CRITERIA

- [x] CFG-9_AC-1 [ubiquitous]: The system SHALL allow undeclared events (with no params), SHALL report a warning for a declared event no state uses, and SHALL make the ref param required on every event type with a `setRef` transition

DEPENDS ON: CFG-6, CFG-7

### CFG-10: Meta keys [MUST]

AS A machine author, I WANT a fixed set of `meta` keys, SO THAT smllm data is unambiguous.

ACCEPTANCE CRITERIA

- [ ] CFG-10_AC-1 [ubiquitous]: The system SHALL accept state `meta` keys `entryPoint`, `paramDescriptions`, `fallback` and machine `meta` keys `smllm`, `instance`, `events`, `sharedActions`, and no others

DEPENDS ON: CFG-1

### CFG-11: Shared actions [SHOULD]

AS A machine author, I WANT actions added to several states at once, SO THAT common instructions are written once.

ACCEPTANCE CRITERIA

- [x] CFG-11_AC-1 [ubiquitous]: The system SHALL apply `meta.sharedActions` in file order, `before` or `after` each named state's own `entry`/`exit`, keeping every occurrence

### CFG-12: Final states [MUST]

AS A machine author, I WANT `type: final` states, SO THAT instances can complete.

ACCEPTANCE CRITERIA

- [x] CFG-12_AC-1 [ubiquitous]: The system SHALL treat final states as optional, SHALL report an error for a final state with `on` or `always`, a finding for each unreachable final state, and an info finding when a machine has no final state

### CFG-13: Structural checks [MUST]

AS A machine author, I WANT structural mistakes caught at load time, SO THAT a machine cannot get stuck at run time.

ACCEPTANCE CRITERIA

- [x] CFG-13_AC-1 [ubiquitous]: The system SHALL report an error when the last transition of a guarded array or `always` list has a guard, when `always` transitions form a loop, when the ref param clashes with `enter`'s own params (`stateMachine`, `state`), when a state uses a built-in event name in `on`, and when more than one state is `meta.fallback` or the fallback state is final — an event param named `stateMachine`/`state`, or named like the ref param on an event without `setRef`, is a warning

### CFG-14: Findings [MUST]

AS A machine author, I WANT every problem reported at once with its location and a fix, SO THAT I fix a file in one pass.

ACCEPTANCE CRITERIA

- [x] CFG-14_AC-1 [ubiquitous]: The system SHALL collect all findings of a file rather than stop at the first, each with file, line, YAML path, level (error, warning, info), message, fix hint where one exists, and rule id

### CFG-15: JSON Schema, template and config combining [MUST]

AS A machine author, I WANT a JSON Schema, a starting file and combined user and project configs, SO THAT editors can check my file and my personal machines work in every project.

ACCEPTANCE CRITERIA

- [x] CFG-15_AC-1 [ubiquitous]: The system SHALL generate the format's JSON Schema from the Rust source types (`smllm info schema`) and SHALL provide a starting template `<id>.smllm.yaml`
- [x] CFG-15_AC-2 [event]: WHEN a user `config.toml` and a project `config.toml` are both loaded THEN the system SHALL combine their machines, the project's machine winning on an `id` clash with an info finding, and the project's `[idle]` replacing the user's — D26

### CFG-16: Tool param types [MUST]

AS A host, I WANT params to be strings only, SO THAT v1 validation stays simple.

ACCEPTANCE CRITERIA

- [x] CFG-16_AC-1 [ubiquitous]: The system SHALL expose tool `params` as `{[name]: string}` and SHALL reject a non-string value naming the param — other JSON Schema types may come later

### CFG-17: Literal braces [MUST]

AS A machine author, I WANT braces in my text shown as written, SO THAT code samples in prompts are not mangled.

ACCEPTANCE CRITERIA

- [ ] CFG-17_AC-1 [ubiquitous]: The system SHALL pass all user-written text through unchanged; nothing in it is treated as a template

## Assumptions

- State machine files are trusted like a Makefile (NFR-5)
- Prompt file paths are relative to the machine file; `config.toml` paths are relative to the config file

## Constraints

- YAML parsing in `smllm-format` only; the lowered model lives in no_std `smllm-core`
- Keys in machine files are camelCase; keys in `config.toml` are kebab-case

## Out of Scope

- Nested and parallel states (v1)
- History states (v1)
- Delayed transitions (`after`) (v1)
- Invoked services (`invoke`) (v1)
- Machine context and `assign` (v1)
- Non-string params (v1)

## Change Log

- 0.1.0 (2026-09-25): Initial requirements from PLAN-001 §3
