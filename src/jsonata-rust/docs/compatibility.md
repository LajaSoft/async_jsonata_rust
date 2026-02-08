# Compatibility Matrix

This document tracks compatibility of `jsonata_rust` against the reference JSONata engine.

Reference docs:
- <https://docs.jsonata.org/overview>
- <https://docs.jsonata.org/path-operators>
- <https://docs.jsonata.org/programming>

## Summary

| Area | Status | Notes |
|---|---|---|
| Parser (core expression grammar) | Implemented | Tested with simple and complex nested expressions, including function blocks and `~>` chains. |
| Rust value model (`JsonValue`/`JsonataValue`) | Implemented | Supports arrays/objects/functions and JSONata-specific sequence metadata. |
| Builtin function registry wiring | Implemented | Includes sync and async function registration paths. |
| Async higher-order execution (`map/filter/single/foldLeft/each/sift`) | Implemented | Covered by async callable tests including explicit `Pending -> Ready` behavior. |
| Full JSONata evaluator parity | In Progress | Parser + functions exist; full end-to-end evaluator remains to be completed. |
| JS bridge parity (`jsonata-js-rust/native`) | In Progress | Extensive coverage exists; some bridge compatibility shims are still present. |

## Known deviations / temporary behavior

- Some bridge-level compatibility fields are normalized in JS wrappers for error-shape parity.
- Full evaluator-level JSONata behavioral parity is not yet declared complete for all test-suite groups.

## Validation sources used in this repo

- JS hybrid suite (`src/jsonata-js-rust/test/run-test-suite.js`)
- Rust integration harness (`src/jsonata-rust/tests/native_wrapper.rs`)
- Native bridge unit tests (`src/jsonata-js-rust/native/src/lib/regex_match.rs`)
