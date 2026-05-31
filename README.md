# async_jsonata_rs

A pure-Rust, async-first reimplementation of the [JSONata](https://jsonata.org/)
query and transformation language. The engine parses and evaluates JSONata
expressions entirely in Rust — no JavaScript runtime involved — and is published
on crates.io as [`async_jsonata_rust`](https://crates.io/crates/async_jsonata_rust).

## Status

- **Official JSONata test suite: 1651 / 1653 passing.** The two remaining cases
  are test-harness artifacts (a per-case depth/timebox limit the harness doesn't
  enforce, and a last-digit floating-point rounding difference), not engine bugs.
- **Genuinely async.** The evaluator is async end-to-end (`evaluate_async`
  returns a future); user-defined functions can be async and are awaited
  cooperatively — it runs under tokio/async-std or any executor. A synchronous
  `evaluate` facade is also provided.

## Using the crate

```toml
[dependencies]
async_jsonata_rust = "0.3"
```

```rust
use async_jsonata_rust::{Evaluator, JsonValue};
use serde_json::json;

let evaluator = Evaluator::with_builtins();
let expr = evaluator.parse("$sum(items.(price * qty))")?;

let input = JsonValue::from_serde_json(&json!({
    "items": [{"price": 10, "qty": 2}, {"price": 5, "qty": 3}]
}));

// Async API (await it on your runtime); a sync `evaluate` exists too.
let out = futures::executor::block_on(evaluator.evaluate_async(&expr, &input))?;
assert_eq!(out.to_serde_json(), Some(json!(35)));
# Ok::<(), async_jsonata_rust::Error>(())
```

You can register custom (including async) functions via `FunctionRegistry`. See
`examples-app/` for a fuller demo: a single complex expression building a report
over an orders document, using aggregation, sorting, higher-order functions,
lambdas, and a user-registered async `$discount` function.

## Repository layout

- `src/jsonata-rust/` — the `async_jsonata_rust` crate (parser, async evaluator,
  function registry, public API).
- `src/jsonata/` — upstream JSONata `LICENSE` + `test/test-suite/` only (the
  JSON test cases used as the behavioural oracle).
- `examples-app/` — a standalone crate that depends on the **published** crate
  from crates.io and runs the demo.
- `Dockerfile` / `compose.yml` — a minimal Rust dev container.

## Development

The dev container ships the Rust toolchain (the host toolchain is not used).

```bash
# Build the dev image
docker compose build dev

# Run all tests (lib + integration + doc; includes the full official suite)
docker compose run --rm --workdir /workspace/src/jsonata-rust dev \
  bash -lc 'export PATH=/workspace/.cargo/bin:/opt/rust/bin:$PATH && cargo test'

# Run the official suite with a per-group summary (optionally pass a group name)
docker compose run --rm --workdir /workspace/src/jsonata-rust dev \
  bash -lc 'export PATH=/workspace/.cargo/bin:/opt/rust/bin:$PATH && cargo run --quiet --example run_suite'

# Run the crates.io demo app (pulls the published crate, not the local sources)
docker compose run --rm examples-app
```

The official suite is also wired as a regular `cargo test`
(`tests/official_suite.rs`), so completeness is checked in CI and new upstream
cases are picked up automatically.

## License

MIT. The bundled JSONata test suite under `src/jsonata/test/test-suite` retains
its original license (`src/jsonata/LICENSE`).

## Acknowledgements

- [JSONata](https://jsonata.org/) — the original language, specification, and
  test suite, maintained by JSONata Limited.
