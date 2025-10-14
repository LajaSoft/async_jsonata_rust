# async_jsonata_rs :: Agent Playbook

## Mission
Rebuild the full JSONata reference implementation in Rust while preserving behavioural parity with the upstream JavaScript project. Use the official JSONata conformance suite as the oracle, replacing the JS internals from the bottom up with native Rust components until the entire stack is Rust-driven.

## Repository Layout
- `src/jsonata/`: pristine copy of upstream JSONata (keep read-only, sync directly from the upstream repo).
- `src/jsonata-js-rust/`: playground where JS modules will be patched to call into Rust via FFI while retaining upstream structure.
- `src/jsonata-rust/`: home for the native Rust crate, tests, and future WASM build artifacts.
- `Dockerfile`: base image with Node.js 20, Rust toolchain, and pnpm preconfigured.
- `compose.yml`: Docker Compose service wiring the dev container with workspace mounts and shared cargo cache.

## Operating Principles
- **Parity first:** Every Rust substitution must satisfy the existing JSONata test corpus before proceeding upward in the stack.
- **Dual runtime:** Maintain a hybrid JS/Rust runtime so that we can progressively swap components without breaking the public Node.js API.
- **Observability:** Record benchmarks and compliance deltas at each replacement stage.
- **Regression guards:** Add Rust-side unit/property tests mirroring any upstream scenario that previously failed.

## Workflow Phases
1. **Bootstrap**
   - Keep `src/jsonata` in lockstep with the upstream repository (no edits).
   - Mirror the upstream tree into `src/jsonata-js-rust` where patches can be applied to call Rust shims without breaking layout.
   - Scaffold the Rust crate inside `src/jsonata-rust` exposing a C-compatible surface callable from Node via `napi-rs` (preferred) or `neon` fallback.
   - Build the dev container (`docker compose build`) to obtain a reproducible Node+Rust toolchain.
   - Baseline by running the upstream JS test suite unchanged inside the container (`docker compose run --rm dev pnpm test`).
2. **Atomics Layer**
   - Identify the smallest pure functions (numbers, strings, time, comparators, math ops).
   - Re-implement each atomic in Rust with comprehensive unit tests.
   - Replace the corresponding JS helper with an FFI call into Rust; prove the upstream spec tests remain green.
3. **Runtime Services**
   - Port data model utilities (path navigation, dynamic object creation, iterators).
   - Implement async/event-loop aware execution in Rust leveraging `tokio` (or `async-std`) to model JSONata's promise handling.
   - Introduce integration tests that stress concurrency and streaming inputs.
4. **Expression Engine**
   - Translate the parser to Rust (consider `pest`/`lalrpop`) while snapshotting AST compatibility.
   - Build the evaluator in Rust, swapping out JS interpreter components module-by-module.
   - Track feature flags for partially ported functionality; keep JS fallbacks until a Rust module hits parity.
5. **Full Rust Mode**
   - Remove JS fallbacks once all major subsystems (lexer, parser, evaluator, built-ins, async runtime) are Rust-complete.
   - Expose a pure Rust API (no Node dependency) plus a WASM target.
   - Recreate the high-level acceptance tests in Rust (`cargo test jsonata_acceptance`) mirroring the JS suite semantics.
6. **Polish & Publish**
   - Document the migration path and usage examples for Rust and Node consumers.
   - Establish CI covering Rust tests, JS compatibility suite, formatting, and benchmarks.
   - Publish the crate to crates.io and provide an npm wrapper pointing to the Rust core.

## Automation Hooks
- `scripts/sync_upstream.sh`: refreshes `src/jsonata` from upstream and re-clones into `src/jsonata-js-rust`.
- `scripts/run_conformance.sh`: executes the upstream JS conformance suite against the hybrid runtime inside Docker.
- `cargo xtask coverage`: drives code coverage + regression summary.

## Status Tracking
- Maintain a `PORTING.md` matrix mapping JSONata features to Rust modules with state (`pending`, `partial`, `done`).
- Every PR must include: affected layer, tests run, parity proof.

## Open Questions
- Decide between `napi-rs` vs `neon` vs pure WASM bridge for the Node adapter.
- Determine the canonical async runtime (`tokio` vs `async-std`) before committing to APIs.
- Explore automatic differential testing against the JavaScript interpreter for fuzzed expressions.
