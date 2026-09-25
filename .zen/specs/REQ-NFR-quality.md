# Requirements Specification

## Introduction

Non-functional requirements for smllm: portability of the core, latency, determinism, safety of hooks and commands, compatibility, code size and maintainability. Source: PLAN-001 §10 (NFR-1..9), decisions D17, D24, D25. Test layers are in REQ-TEST.

## Glossary

- HOST TRAITS: the core's `Store`, `Guard`, `Action`, `InstructionSource`, `Matcher`, `Clock`, `Ids`
- HOOK LATENCY: wall time of one `smllm harness hook` process, excluding guard and action commands

## Stakeholders

- AGENT USER: waits on every hook
- BROWSER HOST AUTHOR: ships `smllm-wasm` in a page
- MAINTAINER: keeps the code within its rules

## Requirements

### NFR-1: Portable core [MUST]

AS A browser host author, I WANT an engine with no IO of its own, SO THAT it runs anywhere.

ACCEPTANCE CRITERIA

- [ ] NFR-1_AC-1 [ubiquitous]: `smllm-core` SHALL be `no_std` + `alloc`, follow the WASM rules in `.zen/rules/rust-rules.md`, and do no IO, time or randomness itself; hosts SHALL supply them through the host traits

### NFR-2: Hook latency [MUST]

AS AN agent user, I WANT hooks that feel instant, SO THAT smllm never slows the agent.

ACCEPTANCE CRITERIA

- [ ] NFR-2_AC-1 [ubiquitous]: A hook SHALL complete in under 50 ms excluding guard and action commands — measured ~1–2 ms with the release binary; no automated check

### NFR-3: Deterministic engine [MUST]

AS A maintainer, I WANT the same inputs to give the same outputs, SO THAT behaviour is testable and reproducible.

ACCEPTANCE CRITERIA

- [ ] NFR-3_AC-1 [ubiquitous]: Given the same stored state, input and guard results, the engine SHALL produce the same output and stored state, verified by property tests — see ENG_P-1

### NFR-4: Hooks never wedge the agent [MUST]

AS AN agent user, I WANT hook failures to be harmless, SO THAT the agent can always continue or stop.

ACCEPTANCE CRITERIA

- [ ] NFR-4_AC-1 [ubiquitous]: A hook failure SHALL never block the agent (HOST-7)

DEPENDS ON: HOST-7

### NFR-5: Trust model for commands [MUST]

AS AN agent user, I WANT LLM input kept out of command text, SO THAT the model cannot inject shell code.

ACCEPTANCE CRITERIA

- [ ] NFR-5_AC-1 [ubiquitous]: State machine files SHALL be trusted like a Makefile, and LLM-supplied values SHALL reach guard and action commands only as environment variables (DEC-6), never interpolated into command text

### NFR-6: Safe failures and stable interfaces [MUST]

AS A script author, I WANT failures that change nothing and interfaces that change rarely, SO THAT automation is dependable.

ACCEPTANCE CRITERIA

- [ ] NFR-6_AC-1 [event]: WHEN a command exits 2 THEN every file SHALL be as it was before the command
- [ ] NFR-6_AC-2 [ubiquitous]: Before 1.0, JSON forms, options and exit codes SHALL change only in a minor release

### NFR-7: Maintainability and release [MUST]

AS A maintainer, I WANT small files, shared rules and one release pipeline, SO THAT the code stays reviewable.

ACCEPTANCE CRITERIA

- [ ] NFR-7_AC-1 [ubiquitous]: Every file SHALL be at most 800 lines, code SHALL follow `.zen/rules/rust-rules.md`, and smllm SHALL ship through the existing release pipeline

### NFR-8: WebAssembly build and size [MUST]

AS A browser host author, I WANT a small wasm build checked on every change, SO THAT the core cannot drift from wasm.

ACCEPTANCE CRITERIA

- [ ] NFR-8_AC-1 [event]: WHEN CI runs THEN it SHALL build `smllm-core` for `wasm32-unknown-unknown` without default features, build `smllm-wasm`, and report the `.wasm` size in the job summary
- [ ] NFR-8_AC-2 [conditional]: IF `smllm_wasm_bg.wasm` (after `wasm-opt -Oz`) exceeds 300 KiB THEN the build SHALL fail — budget set after P3 at ~15% over the first measured 258 KiB

### NFR-9: Unsupported host kinds [MUST]

AS A browser host author, I WANT to know at load time which guards or actions my host cannot run, SO THAT failures are not discovered mid-session.

ACCEPTANCE CRITERIA

- [x] NFR-9_AC-1 [event]: WHEN a machine is loaded by a host that does not support one of its guard or action types (e.g. `command` in a browser) THEN the system SHALL report a finding

## Constraints

- No new dependency without user approval (`.zen/rules/rust-rules.md`)

## Change Log

- 0.1.0 (2026-09-25): Initial requirements from PLAN-001 §10
