# async_jsonata_rust

Async-first JSONata library for Rust: parser, runtime primitives, async custom functions, and stable product-facing API.

## Crate contract

### Scope
- `parser`: JSONata expression parsing into AST JSON (`serde_json::Value`), including recover mode.
- `evaluator`: stable API exists (`Evaluator`), full runtime parity is in progress.
- `async custom functions`: supported through `JsonCallable`/`JsonataCallable` and async operators (`map`, `filter`, `single`, `foldLeft`).
- `compatibility level`: parser + runtime primitives are production-oriented, full end-to-end evaluator parity is tracked explicitly.

### Stable entry points
- `Parser`
- `Expression`
- `Evaluator`
- `FunctionRegistry`
- `Error`

## Quick start (parse)

```rust
use async_jsonata_rust::Parser;

let parser = Parser::new();
let expr = parser.parse("Account.Order[0].Product")?;
println!("AST kind: {}", expr.ast()["type"]);
# Ok::<(), async_jsonata_rust::Error>(())
```

## Evaluation example (what works now)

```rust
use async_jsonata_rust::types::{FunctionContext, JsonValue};
use async_jsonata_rust::{Evaluator, FunctionRegistry};
use futures::executor::block_on;

let evaluator = Evaluator::with_builtins();
let expr = evaluator.parse("$sqrt(81)")?;

// Stable evaluator API is available, runtime parity is still in progress.
let eval_status = evaluator.evaluate(&expr, &JsonValue::Null).unwrap_err();
assert_eq!(eval_status.code(), "E0001");

// Runtime primitives already execute built-ins and async custom functions.
let registry = FunctionRegistry::with_builtins();
let sqrt = registry.get("sqrt").unwrap().clone();
let value = block_on(sqrt.call(FunctionContext::empty(), vec![JsonValue::Number(81.0)]))?;
assert_eq!(value, JsonValue::Number(9.0));
# Ok::<(), Box<dyn std::error::Error>>(())
```

Runnable examples:
- `examples/basic_eval.rs`
- `examples/async_function.rs`
- `examples/custom_registry.rs`
- `examples/error_handling.rs`

## Docs and links
- Crate docs on `docs.rs` will appear after first publish.
- Current source docs live in this repo under `src/jsonata-rust/`.

JSONata references:
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

Detailed matrix: `docs/compatibility.md`.

## Known deviations
- Full evaluator parity with `jsonata-js` is not claimed yet.
- `Evaluator::evaluate` currently returns `E0001` until runtime parity lands.
- Some bridge-focused compatibility shims remain in `jsonata-js-rust/native`.

## MSRV
- `rust-version = 1.78`.
- MSRV bumps are done only in minor/major releases and logged in `CHANGELOG.md`.

## QA gates
- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets --all-features`
- `cargo test --doc --all-features`

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
