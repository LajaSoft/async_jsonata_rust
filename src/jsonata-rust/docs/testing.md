# Testing Strategy

## Layers
1. Golden tests: fixtures with `input` / `expr` / `expected` JSON (`tests/golden/*.json`).
2. Parser/runtime integration tests: `tests/native_wrapper.rs`.
3. Async behavior tests: `tests/async_runtime.rs`.
4. Regression tests by bug-id: `tests/regressions.rs`.
5. Differential checks vs `jsonata-js`: `tests/golden_suite.rs` (`#[ignore]`, requires Node.js).

## Quality gates
- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets --all-features`
- `cargo test --doc --all-features`

## Differential execution
Run manually when Node.js environment is available:
`cargo test differential_matches_jsonata_js_reference -- --ignored`
