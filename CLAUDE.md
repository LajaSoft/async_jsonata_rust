# CLAUDE.md

Guidance for Claude Code when working in this repository.

## Project Overview

A pure-Rust, async-first reimplementation of the [JSONata](https://jsonata.org/)
query language, published on crates.io as **`async_jsonata_rust`**. The engine
parses and evaluates JSONata expressions entirely in Rust, with an async
evaluator (`evaluate_async`) so it composes with any async runtime and can call
user-defined async functions.

There are **no JavaScript sources** in this repo anymore. The only thing kept
from upstream JSONata is its test suite (JSON cases), used as the behavioural
oracle.

## Status

- Official JSONata test suite: **1651 / 1653** passing (the 2 misses are test
  harness artifacts — a per-case depth/timebox limit this harness doesn't
  enforce, and a last-digit f64 rounding difference — not engine bugs).
- The whole evaluator is async (`eval` returns `BoxFuture`); there is no
  `block_on` inside the evaluator and no thread-per-call. A thin sync facade
  (`Evaluator::evaluate`) wraps the async core with a single boundary `block_on`.

## Repository Layout

- `src/jsonata-rust/` — the crate (`async_jsonata_rust`). Parser, evaluator,
  function registry, public API.
- `src/jsonata/` — **only** `LICENSE` + `test/test-suite/` (upstream JSONata
  test cases + datasets), kept as the test oracle. Do not add code here.
- `examples-app/` — a standalone crate that depends on `async_jsonata_rust` from
  crates.io and runs the `orders_report` demo (proves the published crate works
  end-to-end). Run via `docker compose run --rm examples-app`.
- `Dockerfile` / `compose.yml` — minimal Rust dev container (no Node anymore).

### Crate internals (`src/jsonata-rust/src/`)
- `api.rs` — public facade: `Parser`, `Expression`, `Evaluator`,
  `FunctionRegistry`, `evaluate` / `evaluate_async`.
- `parser/` — tokenizer + parser producing an AST as `serde_json::Value`.
- `evaluator/` — async evaluator: `path.rs`, `ops.rs`, `expressions.rs`,
  `transform.rs`, `callable.rs`, `lambda.rs`, `signature.rs`, `value.rs`.
- `functions/` — pure built-in implementations (`core`, `math`, `strings`,
  `regex`, `datetime`, `errors`).
- `registry/` — wires the built-ins into the runtime registry via
  `BuiltinCallable::sync_fn` / `async_fn`.
- `types.rs` — `JsonValue`, `JsonError`, callables, and
  `JsonValue::from_serde_json` / `to_serde_json`.

## Development Commands

All commands run through the Docker dev container (the host has no Rust
toolchain). `cargo` lives at `/opt/rust/bin`.

```bash
# Build the dev image
docker compose build dev

# Run the full test suite (lib + integration + doc tests). This includes
# tests/official_suite.rs, which runs all 1653 official cases (~2 min).
docker compose run --rm --workdir /workspace/src/jsonata-rust dev \
  bash -lc 'export PATH=/workspace/.cargo/bin:/opt/rust/bin:$PATH && cargo test'

# Run the official suite via the dev harness with a per-group summary
docker compose run --rm --workdir /workspace/src/jsonata-rust dev \
  bash -lc 'export PATH=/workspace/.cargo/bin:/opt/rust/bin:$PATH && cargo run --quiet --example run_suite'

# ...or a single group with per-case failure detail
docker compose run --rm --workdir /workspace/src/jsonata-rust dev \
  bash -lc 'export PATH=/workspace/.cargo/bin:/opt/rust/bin:$PATH && cargo run --quiet --example run_suite -- function-sort'

# Build/run the crates.io demo app
docker compose run --rm examples-app
```

## Testing model

- `tests/official_suite.rs` is the canonical regression guard: it loads every
  case under `src/jsonata/test/test-suite`, evaluates it through the pure-Rust
  `Evaluator`, and asserts per-group pass floors plus a total floor. A new
  failure (regression, or a newly added upstream case) trips it.
- `examples/run_suite.rs` is the same logic as a CLI for day-to-day TDD
  (optionally filtered to one group). Keep the two comparison rules in sync.
- Other tests: `async_evaluate.rs` (async API + a yielding user fn),
  `async_runtime.rs`, `native_wrapper.rs`, `golden_suite.rs`, `regressions.rs`.

## Working notes

- When adding/fixing a built-in: implement in `functions/<area>.rs`, wire it in
  `registry/<area>.rs`, then drive the relevant `function-*` suite group to
  green via the harness.
- Keep higher-order functions async (`async_fn` + `BoxFuture`); never add
  `block_on`/`thread::spawn` inside `evaluator/`.
- The upstream reference for exact semantics/error codes is the JSONata docs
  (<https://docs.jsonata.org/>); the JS source is no longer in the repo.
- Publishing: bump `version` in `src/jsonata-rust/Cargo.toml`, then
  `cargo publish` from that directory (crates.io won't accept a re-used
  version).
