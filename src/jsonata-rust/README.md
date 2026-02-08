# jsonata_rust

Async-first Rust implementation of JSONata parser/runtime building blocks.

This crate is part of `async_jsonata_rs` and is focused on:
- parser compatibility with reference JSONata syntax,
- Rust-native value/function model,
- async function execution primitives,
- stable foundation for a full evaluator.

JSONata syntax and semantics reference:
- <https://docs.jsonata.org/overview>
- <https://docs.jsonata.org/path-operators>
- <https://docs.jsonata.org/programming>

## Status

Current maturity: parser + function/runtime infrastructure are production-oriented.
Full evaluator parity is still in progress.

Detailed compatibility matrix:
- `docs/compatibility.md`

## Quick Start

```rust
use jsonata_rust::parse_expression;

let ast = parse_expression("Account.Order[0].Product", false)?;
assert!(ast.is_object());
# Ok::<(), jsonata_rust::ParserError>(())
```

## Async callable integration

`functions::core` APIs accept `JsonValue::Function` and execute async callables.
See runnable example:
- `examples/async_map.rs`

## Running checks

```bash
cargo test -- --nocapture
cargo test --test native_wrapper -- --nocapture
cargo test --examples
```

## MSRV

- Rust `1.78`

## License

MIT
