# Requirements Specification

## Introduction

The test layers smllm must have. Source: PLAN-001 §10 Testing (TEST-1..4), decision D30. How each layer is built is in DESIGN-NFR-quality.

## Glossary

- SCRIPTED SESSION: a test that plays a fake agent, feeding hook JSON and tool calls to the real binary
- LIVE TEST: a test that runs a real model through Claude Code

## Stakeholders

- MAINTAINER: relies on the suite before every merge and release
- HUMAN REVIEWER: decides when the live test is worth its cost

## Requirements

### TEST-1: Core tests [MUST]

AS A maintainer, I WANT unit, snapshot and property tests of the core, SO THAT engine behaviour and agent text are pinned.

ACCEPTANCE CRITERIA

- [ ] TEST-1_AC-1 [ubiquitous]: The suite SHALL unit-test the core, snapshot all agent text (entry blocks, events lists, errors, tool description, instructions block), and property-test that an invalid call never changes state, determinism, monotonic visits, and never resting in an `always` state — properties are ENG_P-1..4; the tool description and instructions block are snapshots in the app

### TEST-2: Scripted sessions [MUST]

AS A maintainer, I WANT a fake agent to drive the real binary, SO THAT hooks, `fire` and MCP work together end to end.

ACCEPTANCE CRITERIA

- [ ] TEST-2_AC-1 [ubiquitous]: The suite SHALL drive `harness hook claude …` JSON, `fire` and MCP over stdio through the showcase and a `dev` example, covering enter, setRef, guarded transitions (true and false), park, detour/resume, takeover, final and reopen — showcase covered; `dev` is driven only through wasm (TEST-3); reopen is covered only in core tests

### TEST-3: WebAssembly tests [MUST]

AS A browser host author, I WANT the wasm package exercised from JavaScript, SO THAT the published API works.

ACCEPTANCE CRITERIA

- [x] TEST-3_AC-1 [ubiquitous]: CI SHALL load the built `smllm-wasm` in Node and run one scripted session through the JS API

### TEST-4: Live-model test [SHOULD]

AS A human reviewer, I WANT an opt-in test with a real model, SO THAT the full loop is checked without spending tokens by accident.

ACCEPTANCE CRITERIA

- [x] TEST-4_AC-1 [complex]: The project SHALL provide `scripts/live-e2e.mjs`, which drives `claude -p` through a machine and checks the recorded history; it SHALL refuse to run unless `SMLLM_LIVE=1` and SHALL run only on explicit human request, never in CI or hooks — run once on request 2026-09-25 (haiku), passed

## Constraints

- Tests use isolated user directories (`HOME`, `XDG_CONFIG_HOME`, `XDG_STATE_HOME`) and never touch the developer's own

## Change Log

- 0.1.0 (2026-09-25): Initial requirements from PLAN-001 §10
