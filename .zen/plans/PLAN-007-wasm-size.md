# PLAN-007: Smaller WASM build

| Meta               | Value                                                         |
| ------------------ | ------------------------------------------------------------- |
| Status             | completed (S1–S4, 2026-09-28) |
| Workflow direction | bottom-up (measure → decisions → code → specs)                |
| Traces to          | NFR-8, HOST-Wasm (DESIGN-HOST), DESIGN-ENG (serde feature), D6-2 (reproducible output) |

## 1. Goal

Cut `smllm_wasm_bg.wasm` (after `wasm-opt -Oz`) from 263,192 bytes (257 KiB; 111 KiB gzip), keeping the API
(PLAN-006 D6-3) and the smoke tests unchanged, then lower the NFR-8 budget to the new size.

## 2. Where the bytes go (2026-09-28, stable 1.98.1)

Code 227 KB, data 38 KB. Measured with a named build (`strip = false`, `wasm-opt -g`), twiggy and a
per-function tally.

| Share | What |
| ----- | ---- |
| ~87 KB (38%) | serde + serde_json decoding: a copy of the decoder per type (Machine, State, Instance, Session…) |
| 14 KB | `f64` Display (+ ~3 KB flt2dec tables): only reachable from serde's `Unexpected::Float` error text inside serde_json; no float is ever printed |
| 7 KB | dlmalloc (std's wasm allocator) |
| ~6 KB | `serde_json::Value` + indexmap + hashbrown (`preserve_order`), used only for `fire` params, env and `stop` |
| ~5 KB | decoding `history` on `importState`, which WASM never holds since D6-4 |
| ~20 KB | engine logic (`idle::fire`, `idle::list`, turn, offers): the product |
| ~20 KB strings | agent text, serde/panic messages, **absolute build paths** (`/home/vscode/.rustup/…`, `…/registry/src/…`) in panic locations |

## 3. Items

Sizes are wasm bytes after `-Oz`, each measured on top of the rows above it unless marked alone.

| ID | Change | Measured | Cost / risk | Status |
| -- | ------ | -------- | ----------- | ------ |
| W-1 | smllm-wasm drops `serde_json::Value`: `fire` params as `SmallMap<String>`, env serialized by a borrowing wrapper, `stop` as a derived struct | −11,689 | none; keep the "param k must be a string" text | subsumed by D7-1 |
| W-2 | smllm-wasm's own `serde_json` dep without `preserve_order`/`std` (`alloc` only) | −3,336 | none (key order comes from `SmallMap`) | subsumed by D7-1 |
| W-3 | `importState` decodes sessions, bindings, instances only (not `history`) | −5,672 | none | subsumed by D7-1 |
| W-4 | hand-written `Deserialize` for untagged `model::Value` (no serde content buffering) | −4,460 | ~35 lines in core; same accepted inputs | subsumed by D7-1 |
| W-5 | Replace serde/serde_json in the WASM build with **miniserde** or **nanoserde** (core feature beside `serde`, which the CLI keeps); see §3.1 | miniserde −81,284 (→157,106; gzip 72,608); nanoserde −42,384 (→196,006) | new dependency; hand impls; error detail | decided (D7-1) |
| W-6 | Nightly `-Zbuild-std=std,panic_abort` + `optimize_for_size` + `-Cpanic=immediate-abort` | −32,353 on serde (−13,613 build-std, −1,644 size feature, −16,962 immediate-abort); on miniserde −20,243 (→136,863; gzip 64,139); on miniserde + talc −17,339 (→132,687; gzip 62,325) | nightly toolchain pinned in CI/release for the wasm job only; a panic becomes a bare `unreachable` trap (no message) | decided (D7-3) |
| W-7 | Remove absolute paths from the binary: `--remap-path-prefix` of `$HOME` and the checkout in `build-wasm.mjs` (W-6 drops all but one location on its own; `-Zlocation-detail=none` −1,111 on nightly) | −68 on miniserde; leaves only `/rustc/<commit>/…` (the same for one toolchain) | none | decided (D7-4) |
| W-8 | A smaller allocator (e.g. `talc`) instead of dlmalloc | talc 5.1.1: −6,840 on miniserde (→150,266); with `disable-realloc-in-place` −7,080 (→150,026; gzip 69,907); smoke tests pass (`exp/miniserde-talc`) | new dependency (talc + allocator-api2, lock_api); allocator correctness for a long-lived engine; the README claims it is faster than dlmalloc | rejected (D7-2) |
| W-9 | `opt-level = "s"` for the wasm build | +16,744 on miniserde (→173,850) | — | rejected |
| W-10 | wasm-opt flags: `-Oz --converge` −142, `-O4 -Oz` +3,208 | measured | — | rejected (noise) |
| W-12 | More Binaryen `wasm-opt` (v132) passes, on miniserde (157,106): `--strip-producers --strip-target-features` −74; `--zero-filled-memory --low-memory-unused` −642; all with `--converge` −866; `--flatten --rereloop -Oz` −1,091; `--gufa`, `--merge-similar-functions`, `-Oz` ×3 ≤ −145 | measured | stripping the two custom sections is free (they hold only toolchain names and features); the memory flags assume a layout detail (nothing below address 1024) for 0.4%; flatten/rereloop costs build time and may cost speed | to decide |
| W-11 | Shared text: `f64` Display and serde's error strings vanish only with W-5 (or a custom JSON reader) | follows W-5 | — | depends on W-5 |

Stacked: serde 263,192 → W-1..W-4 238,390 → W-5 miniserde 157,106 → W-8 talc 150,026 → W-6 132,687
(−49.6%; gzip 113,775 → 62,325, −45%). W-7 is size-neutral.

W-1..W-4 together: 263,192 → 238,035 (−25,157, −9.6%; gzip 113,775 → 103,247). On the prototype
baseline (`exp/serde-baseline`, W-1..W-4 plus a hand params reader keeping "param k must be a
string"): 238,390.

### 3.1 W-5: miniserde vs nanoserde (prototypes, 2026-09-28)

Both: smoke tests pass unmodified; no serde/serde_json in the wasm build (`cargo tree`); a core test
with both features on shows `dev.json` and Session / Instance / HistoryEntry / MemoryStore / Value
encode byte-identically to serde and read back equal.

| | miniserde 0.1.46 | nanoserde 0.2.1 (`json`, no `std`) |
| - | - | - |
| wasm / gzip | **157,106 / 72,608** (−34% / −30%) | 196,006 / 87,015 (−18% / −16%) |
| Hand code | ~560 lines (core `mini.rs` 412, `SmallMap` 65, wasm ~80) + ~30 `rename`s | ~256 lines (core `nano.rs` 216, wasm ~40) + ~55 attribute lines |
| Cannot derive | data enums (GuardDef, Prompt, ActionDef), untagged Value, SmallMap, structs with `default` fields (Config, Session, Instance, HistoryEntry), MemoryStore | newtype variants (Prompt, ActionDef: arrays), untagged Value, SmallMap, `skip_serializing_if`, `r#ref` field (broken codegen) |
| no_std | yes (alloc) | yes, no dependencies |
| Error text | always "miniserde error" | readable, with line and column |
| Strictness | as strict as serde on every probe; no recursion (100k nesting fine); duplicate fields: last wins | lenient (comments, trailing commas, `\q`, control chars, trailing junk); **traps** (`unimplemented!`) or **hangs** on some malformed JSON; wraps u64 > i64::MAX; writes DEL as `\u007f` |
| Maintenance | dtolnay, 2026-07; 7M downloads; "prototype, feature requests rejected"; deps itoa, zmij, mini-internal (syn 3, already present) | last release 2025-03; 1.7M downloads |

Prototype's recommendation: miniserde, with context around its error (e.g. "invalid compiled JSON:
…" / "invalid state JSON") and the equivalence test in CI so the two derives cannot drift.
nanoserde is out: `Engine(compiled)` and `importState` take host input, which must never trap or
hang the engine (PLAN-003 F22).

## 4. Phases (after the decisions)

Each phase one commit, `npm run -s build:wasm`, `npm run -s test:wasm` and `npm run -s test:gate` passing.

| Phase | Items | Proof |
| ----- | ----- | ----- |
| S1 | D7-1: miniserde (W-5, folding in W-1..W-4), from `exp/miniserde`, reviewed and tidied | smoke tests unchanged; `dev.json` (serde-written) loads; the core equivalence test (`--features serde,miniserde`) runs in the gate; malformed input gives a contextual error; ~157 KB |
| S2 | D7-4: path remap (W-7) | no `/home/` or registry path in the `.wasm` |
| S3 | D7-3: build-std on a pinned nightly (W-6) | `build:wasm`, CI and release on the pinned nightly; ~137 KB |
| S4 | NFR-8 budget → new size + ~15%; DESIGN-HOST, DESIGN-ENG, ARCHITECTURE, CHANGELOG, §6 | — |

## 5. Decisions

- D7-1 (W-5, W-1..W-4, W-11): the WASM build uses miniserde: smllm-core gains a `miniserde` feature
  beside `serde` (the CLI keeps serde, which serde-saphyr and schemars need); smllm-wasm drops serde
  and serde_json. The JSON stays byte-identical to serde's, proven by a core test built with both
  features and run by the gate, so the two cannot drift. miniserde's bare "miniserde error" gets
  context in smllm-wasm ("invalid compiled machines", "invalid state", "param k must be a
  string"). nanoserde rejected: it traps or hangs on some malformed JSON (§3.1), and host input
  must never break the engine (PLAN-003 F22). W-1..W-4 are subsumed (serde_json leaves the wasm
  build; history is still not read on `importState`).
- D7-2 (W-8): keep std's allocator (dlmalloc), for safety: the ~7 KB is not worth a second
  allocator under a long-lived engine.
- D7-3 (W-6): `build:wasm` builds with `-Zbuild-std=std,panic_abort`,
  `-Zbuild-std-features=optimize_for_size` and `-Cpanic=immediate-abort` on one dated nightly,
  pinned in `scripts/build-wasm.mjs` (with `rust-src` and the wasm32 target) and installed by the
  CI and release wasm jobs and `.mise.toml`. The CLI and the no_std core check stay on stable
  1.98. A panic becomes a bare trap; the engine reports host failures without panicking
  (PLAN-003 F22), so none is expected.
- D7-4 (W-7): `build:wasm` remaps `$HOME` and the checkout out of the binary's paths, so the
  package holds no path of the machine that built it (as D6-2 for `compile`).
- D7-5 (W-12): `wasm-opt` adds `--strip-producers --strip-target-features` (free; they name the
  toolchain). Not the memory-layout flags (0.4%, an assumption a toolchain change could break) nor
  `--flatten --rereloop` (build time, speed).
- Rejected: W-9 (`opt-level = "s"`, +16.7 KB), W-10 (wasm-opt flags, noise).

## 6. Outcome

| Phase | Commit / result |
| ----- | --------------- |
| S1 | 83111b4: core `miniserde` feature (`src/mini.rs`, derives with `serde(rename)`), smllm-wasm on miniserde only; serde's `Value` decoder stays derived (W-4 was only for the serde wasm). Tests with both features (run by the gate through feature unification): `dev.json` and every record, guard, action, prompt, position and status encode byte-identically and read back equal; 13 malformed inputs both reject. Smoke test: malformed constructor, `importState` and params give "invalid compiled machines", "invalid state", "params must be a JSON object". 157,081 bytes |
| S2 | a40275c: `build-wasm.mjs` remaps `CARGO_HOME`, the checkout and the toolchain's `rust-src` (to `/rustc/<commit>`, where std's own paths point) and fails if the home, `CARGO_HOME`, checkout or `rust-src` path remains; strips the producers and target-features sections. Only `/rustc/<commit>/…` and `/cargo/registry/…` remain. 156,772 bytes |
| S3 | 6dfb3df: `NIGHTLY = "nightly-2026-09-26"` in `build-wasm.mjs`, which installs it (`rustup toolchain install`, a no-op when present) with `rust-src` and wasm32, so CI, release and local builds share one pin: no workflow change; `.mise.toml` says so (D7-3 said the jobs and mise would install it). 136,735 bytes (133.5 KiB; −48% from 263,192) |
| S4 | budget 154 KiB (NFR-8_AC-2); DESIGN-NFR, DESIGN-ENG, DESIGN-HOST, ARCHITECTURE 0.7.0, rust-rules (keep the two impls in step), CHANGELOG, this section |
