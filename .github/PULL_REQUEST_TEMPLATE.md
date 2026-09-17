## Summary

<!-- What does this change, and why? -->

## Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo nextest run --workspace` passes
- [ ] `cargo test --doc --workspace` passes
- [ ] `npm run test:launcher` and `npm run verify-version` pass
- [ ] `npm run coverage:check` passes (per-crate line coverage >= 90%)
- [ ] Docs updated (README / rustdoc / `CHANGELOG.md`) where behaviour changed
