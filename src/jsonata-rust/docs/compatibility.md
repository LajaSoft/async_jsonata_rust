# JSONata Compatibility Matrix

This document tracks compatibility of `jsonata_rust` against the reference `jsonata-js` engine.

## Reference baseline
- Engine: `jsonata-js`
- Version: `2.1.0`
- Syntax docs:
  - <https://docs.jsonata.org/overview>
  - <https://docs.jsonata.org/path-operators>
  - <https://docs.jsonata.org/programming>

## Coverage matrix

| Capability | Status | Coverage source |
|---|---|---|
| Parser core grammar | Implemented | `tests/native_wrapper.rs`, parser module tests |
| Function/runtime value model (`JsonValue`/`JsonataValue`) | Implemented | `types.rs` + integration tests |
| Built-in registry wiring | Implemented | `registry.rs` + example/tests |
| Async callables and HOF primitives (`map`, `filter`, `single`, `foldLeft`) | Implemented | async tests with explicit `Pending -> Ready` behavior |
| Full evaluator output parity vs JSONata-js test-suite groups | In progress | stable `Evaluator` API exists, runtime parity not complete |
| JS native bridge parity (`jsonata-js-rust/native`) | In progress | dedicated native tests + bridge layer |

## Test groups snapshot

| Group bucket | Status |
|---|---|
| Parser-focused expressions (paths, predicates, function blocks) | Covered |
| Async function behavior | Covered |
| Complex real-world golden expressions | Started |
| Differential parity vs `jsonata-js` executable | Optional (env/CI gated) |
| Full official suite pass/fail dashboard | Planned |

## Known deviations
- `Evaluator::evaluate` currently returns `E0001` while runtime engine parity is under active development.
- Full claim for all official `jsonata-js` test-suite groups is intentionally deferred.
- Bridge-level error-shape shims may exist for JS interop compatibility.

## Validation sources in this repository
- `src/jsonata-js-rust/test/run-test-suite.js`
- `src/jsonata-rust/tests/native_wrapper.rs`
- `src/jsonata-js-rust/native/src/lib/regex_match.rs`
