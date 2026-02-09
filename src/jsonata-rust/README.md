# async_jsonata_rust

Async-first Rust crate for JSONata parser/runtime foundations.

## Crate contract

### Scope
- `parser`: JSONata expression parsing into AST JSON (`serde_json::Value`), including recover mode.
- `evaluator`: stable facade exists (`Evaluator`), full runtime parity is in progress.
- `async custom functions`: supported through `JsonCallable`/`JsonataCallable` traits and async core operators (`map`, `filter`, `single`, `foldLeft`).
- `compatibility level`: parser and function runtime primitives are production-oriented; evaluator parity is explicitly tracked as in-progress.

### Stable entry points
- `Parser`
- `Expression`
- `Evaluator`
- `FunctionRegistry`
- `Error`

Low-level internals are still available under `parser::*`, but product API should use the stable layer above.

## Quick start

```rust
use async_jsonata_rust::Parser;

let parser = Parser::new();
let expr = parser.parse("Account.Order[0].Product")?;
println!("AST kind: {}", expr.ast()["type"]);
# Ok::<(), async_jsonata_rust::Error>(())
```

## JSONata references
- <https://docs.jsonata.org/overview>
- <https://docs.jsonata.org/path-operators>
- <https://docs.jsonata.org/programming>

## Compatibility with JSONata-js

Reference engine: `jsonata-js` `2.1.0`.

| Area / test groups | Compatibility | Evidence |
|---|---|---|
| Parser grammar (paths, predicates, functions, chains) | High | Rust parser tests + JS suite fixtures in repo |
| Built-in runtime helpers (math/core/string primitives) | Medium | `tests/native_wrapper.rs` + function module tests |
| Async function execution (`Pending -> Ready`) | High | async callable tests in `tests/native_wrapper.rs` |
| Full evaluator output parity across all suite groups | In progress | evaluator facade exists, runtime engine not finalized |

Detailed matrix and notes: `docs/compatibility.md`.

## Known deviations
- Full end-to-end evaluator parity with `jsonata-js` is not claimed yet.
- Stable `Evaluator::evaluate` currently returns explicit `E0001` (`not implemented`) until runtime parity lands.
- Some bridge-focused compatibility shims remain in `jsonata-js-rust/native`.

## MSRV policy
- `rust-version = 1.78`.
- MSRV is checked in CI and treated as part of release quality.
- MSRV bumps are allowed only in minor/major releases and documented in `CHANGELOG.md`.

## SemVer policy
- Public API: `Parser`, `Expression`, `Evaluator`, `FunctionRegistry`, `Error`, public modules, and documented behavior.
- Breaking change examples: removing/renaming public items, changing documented error codes/fields, altering stable semantics.
- Internal modules and undocumented internals are not covered by stability guarantees.

## QA / CI gates
- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets --all-features`
- `cargo test --doc --all-features`
- Cross-platform matrix: Linux, macOS, Windows

## Examples
- `examples/basic_eval.rs`
- `examples/async_function.rs`
- `examples/custom_registry.rs`
- `examples/error_handling.rs`

## Roadmap

### Implemented
- Parser with recover mode.
- Async callable model and core async operators.
- Built-in function registry wiring.
- Stable public API facade and unified error type.

### In progress
- Full evaluator runtime parity with JSONata-js.
- Differential test automation against JS reference engine.

### Planned
- Full golden suites by expression groups.
- Cross-engine conformance dashboard.
- Automated release tagging and publish pipeline.

## License
MIT
