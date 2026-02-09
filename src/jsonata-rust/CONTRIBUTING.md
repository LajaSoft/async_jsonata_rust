# Contributing

## Scope
This crate follows a product-style contract documented in `README.md`.
Contributions should preserve stability of public APIs:
- `Parser`
- `Expression`
- `Evaluator`
- `FunctionRegistry`
- `Error`

## Development setup
1. Install stable Rust (MSRV `1.78` or newer).
2. Work in `src/jsonata-rust`.
3. Run quality gates before opening PR.

## Required checks
- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets --all-features`
- `cargo test --doc --all-features`

## Test strategy expectations
- Add/extend golden tests for realistic JSONata expressions.
- Add regression tests named by bug id.
- Add async tests for custom callable behavior when touching async runtime.
- Prefer differential checks against `jsonata-js` where practical.

## SemVer policy for contributors
Treat as breaking:
- Removing or renaming public items.
- Behavioral changes in documented stable APIs.
- Changing error code semantics for documented cases.

## Commit hygiene
- Keep changes small and focused.
- Update `CHANGELOG.md` under `[Unreleased]`.
- Update docs/examples when API behavior changes.
