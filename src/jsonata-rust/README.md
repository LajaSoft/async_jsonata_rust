# async_jsonata_rust

Async-first JSONata library for Rust: parser, runtime primitives, async custom functions, and stable product-facing API.

## Crate contract

### Scope
- `parser`: JSONata expression parsing into AST JSON (`serde_json::Value`), including recover mode.
- `evaluator`: end-to-end expression evaluation is implemented (including `bind`, `lambda`, `apply`, path/filter/sort flows used by current native tests).
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

## Evaluation example

```rust
use async_jsonata_rust::{Evaluator, JsonValue, JsonObject};

let evaluator = Evaluator::with_builtins();
let expr = evaluator.parse(
    "(
      $norm := function($o){$sum($o.items.(price * qty))};
      $map(orders, function($o){{\"id\": $o.id, \"total\": $norm($o)}})
    )"
)?;

let input = JsonValue::Object(JsonObject(vec![(
    "orders".to_string(),
    JsonValue::Array(async_jsonata_rust::JsonArray::new(
        vec![
            JsonValue::Object(JsonObject(vec![
                ("id".to_string(), JsonValue::String("A1".to_string())),
                ("items".to_string(), JsonValue::Array(async_jsonata_rust::JsonArray::new(
                    vec![
                        JsonValue::Object(JsonObject(vec![
                            ("price".to_string(), JsonValue::Number(10.0)),
                            ("qty".to_string(), JsonValue::Number(2.0)),
                        ])),
                        JsonValue::Object(JsonObject(vec![
                            ("price".to_string(), JsonValue::Number(5.0)),
                            ("qty".to_string(), JsonValue::Number(2.0)),
                        ])),
                    ],
                    false,
                    false,
                ))),
            ])),
        ],
        false,
        false,
    )),
)]));

let out = evaluator.evaluate(&expr, &input)?;
println!("{out:?}");
# Ok::<(), async_jsonata_rust::Error>(())
```

Runnable examples:
- `examples/basic_eval.rs`
- `examples/evaluator_end_to_end.rs`
- `examples/evaluator_bind_lambda.rs`
- `examples/async_function.rs`
- `examples/custom_registry.rs`
- `examples/error_handling.rs`
- `examples/registry_usage.rs`

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
| Built-in runtime helpers (math/core/string primitives) | High | `tests/native_wrapper.rs` + function module tests |
| Async function execution (`Pending -> Ready`) | High | async callable tests in `tests/native_wrapper.rs` |
| Full evaluator output parity across all suite groups | In progress | complex evaluator integration tests are green, full suite parity still in progress |

Detailed matrix: `docs/compatibility.md`.

## Known deviations
- Full evaluator parity with `jsonata-js` is not claimed yet.
- Some bridge-focused compatibility shims remain in `jsonata-js-rust/native`.

## Run examples

```bash
cargo run --example basic_parse
cargo run --example basic_eval
cargo run --example evaluator_end_to_end
cargo run --example evaluator_bind_lambda
cargo run --example async_function
```

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
