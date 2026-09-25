# Rust Rules

## Cargo.toml

- All dependency versions, build optimisation, etc, should be hoisted to the workspace Cargo.toml where possible.
- All dependencies should be at their latest versions unless there is a good reason not to be. Check latest version when adding a dependency.
- No dependency will be added without user confirmation.

## Rust Module Rules

- `mod.rs` contains only `mod` declarations and `pub use` re-exports; all code lives in named files.
- Declare submodules privately (`mod foo;`) and expose their API via `pub use foo::Item;`; flatten re-exports at `lib.rs` so callers write `crate::Item`.
- Default to private; widen visibility via `pub(crate)` → `pub(super)` → `pub` only as needed.
- Group imports `std` → external → `crate`/`super`/`self`, collapse with nested paths, and prefer `use crate::...` over `super::super::.`
- Import types and traits directly; import the parent module for free functions (`module::func()`); no glob imports except preludes, enum variants in `match`, and tests.

## WASM Binary Size (`smllm-core` is `no_std`)

`smllm-core` compiles `no_std` + `alloc` for the size-critical wasm target (a browser host runs the state machines; see PLAN-001 D24). `cargo test` links `std` and **masks** `no_std`/size regressions. Verify import- or size-sensitive changes with `cargo build -p smllm-core --no-default-features --target wasm32-unknown-unknown` and `npm run build:wasm` (prints the `smllm_wasm_bg.wasm` size and fails over its budget), not just the test suite. Parsing, validation and everything std-only belong in `smllm-format` or the app, never in the core.

- Import `String`/`Vec`/`format!`/etc. via `use crate::prelude::*;` — never rely on the std prelude (it is absent under `no_std`).
- Do NOT use std's general sort (`.sort()`, `.sort_by`, `.sort_by_key`, `.sort_unstable*`) in `smllm-core`: it pulls ~23 KB of driftsort/ipnsort into the wasm. Use `crate::utils::insertion_sort_by_key` for the engine's small collections.
- Do NOT use `BTreeSet`/`BTreeMap` in `smllm-core`: each element type instantiates the whole B-tree machinery. Use `crate::utils::SmallMap` (a sorted `Vec`, key order, binary search, shifted insert; serialises as a JSON object).
- Use ASCII case ops (`to_ascii_lowercase`/`to_ascii_uppercase`) for ASCII data (names, env var names); `to_lowercase`/`to_uppercase` drag the Unicode case-folding tables into the binary.
- Avoid `{:?}`/`Debug` formatting in non-test code paths of `smllm-core` — it pulls each type's `Debug` impl (and more `core::fmt` machinery) into the release wasm.
- IO, time, randomness, regex and process spawning come from the host traits in `smllm_core::host` (NFR-1); never add a dependency to `smllm-core` that needs `std`.

## Unsafe Rust Code

- all `unsafe` blocks must be isolated in a dedicated module named `*_unsafe.rs` behind safe public functions.
- no unsafe code without user confirmation, even then must be clearly documented.
