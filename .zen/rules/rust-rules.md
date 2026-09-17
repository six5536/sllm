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

## WASM Binary Size (the lib is `no_std`)

The lib compiles `no_std` for the size-critical wasm target; `cargo test` links `std` and **masks** `no_std`/size regressions. Verify import- or size-sensitive changes with `cargo build -p bitmark-parser --no-default-features --target wasm32-unknown-unknown` (and `npm run build:wasm` for the size number), not just the test suite.

- Import `String`/`Vec`/`format!`/etc. via `use crate::prelude::*;` — never rely on the std prelude (it is absent under `no_std`).
- Do NOT use std's general sort (`.sort()`, `.sort_by`, `.sort_by_key`, `.sort_unstable*`): it pulls ~23 KB of driftsort/ipnsort into the wasm. Use `crate::utils::sort::insertion_sort_by_key` for the parser's small collections.
- Do NOT use `BTreeSet`/`BTreeMap` for the parser's small collections: each element type instantiates the whole B-tree machinery (~1.5 KB each), and building one with `.collect()` / `FromIterator` also bulk-builds by sorting (the same std sort). Use `crate::utils::SortedVecSet` / `crate::utils::SortedVecMap` — same `Ord` iteration order, binary search, shifted insert. The one sanctioned exception is `diff/align.rs`'s priority queue, where an O(n) shifted insert would be quadratic (PLAN-182 D2).
- Use ASCII case ops (`to_ascii_lowercase`/`to_ascii_uppercase`) for ASCII data (config names, identifiers, markers); `to_lowercase`/`to_uppercase` drag the Unicode case-folding tables into the binary.
- Avoid `{:?}`/`Debug` formatting on parser types in non-test code paths — it pulls each type's `Debug` impl (and the `core::fmt` machinery) into the release wasm.

## Unsafe Rust Code

- all `unsafe` blocks must be isolated in a dedicated module named `*_unsafe.rs` behind safe public functions.
- no unsafe code without user confirmation, even then must be clearly documented.
