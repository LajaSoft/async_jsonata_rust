# async_jsonata_rust

Async-first JSONata library for Rust: parser, runtime primitives, async custom functions, and stable product-facing API.

## Crate contract

### Scope
- `parser`: JSONata expression parsing into AST JSON (`serde_json::Value`), including recover mode.
- `evaluator`: full async expression evaluation (`evaluate_async`), covering paths/predicates/sort, blocks, `bind`, lambdas (incl. recursion), `apply` (`~>`), partial application, transforms, grouping, and the full built-in set.
- `async custom functions`: supported through `JsonCallable`/`JsonataCallable`; user functions can be async and are awaited cooperatively by the evaluator.
- `compatibility level`: 1651/1653 of the official JSONata test suite (see Compatibility below).

### Stable entry points
- `Parser`
- `Expression`
- `Evaluator` (`evaluate` and `evaluate_async`)
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
- Published on crates.io: <https://crates.io/crates/async_jsonata_rust>
  (API docs on <https://docs.rs/async_jsonata_rust>).
- A standalone usage demo (pulling the crate from crates.io) lives in
  `examples-app/` at the repo root.

JSONata references:
- <https://docs.jsonata.org/overview>
- <https://docs.jsonata.org/path-operators>
- <https://docs.jsonata.org/programming>

## Compatibility with JSONata

Reference: the official JSONata test suite (`jsonata-js` 2.x cases), bundled
under `src/jsonata/test/test-suite` and run against this engine by
`tests/official_suite.rs`.

**Result: 1651 / 1653 cases passing.** The two misses are test-harness
artifacts, not engine differences:
- `tail-recursion` `$factorial(100)` expects a `U1001` depth error that the
  upstream runner produces via a per-case timebox; this harness does not enforce
  that limit.
- `tail-recursion` `$factorial(150)` differs only in the last floating-point
  digit (multiplication-order rounding).

Detailed matrix: `docs/compatibility.md`.

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
- Async evaluator end-to-end (`evaluate_async`), no internal `block_on`.
- Async callable model and higher-order operators.
- Built-in function registry wiring (math, core, strings, regex, datetime, errors).
- Stable public API facade and unified error type.
- Full official test-suite run as a `cargo test` (`tests/official_suite.rs`),
  1651/1653 passing.

### Planned
- Close the remaining recursion/timebox edge cases.
- WASM build target.

## License
MIT
