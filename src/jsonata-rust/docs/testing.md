# Testing Strategy

## Layers
1. Golden tests: fixtures with `input` / `expr` / `expected` JSON (`tests/golden/*.json`).
2. Parser/runtime integration tests: `tests/native_wrapper.rs`.
3. Async behavior tests: `tests/async_runtime.rs`.
4. Regression tests by bug-id: `tests/regressions.rs`.
5. Official JSONata compatibility suite: `tests/official_suite.rs`.

## Quality gates
- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets --all-features`
- `cargo test --doc --all-features`

The former Node.js differential test was removed with the bundled JavaScript
runtime. `tests/golden_suite.rs` now provides parser smoke coverage only.
