# async_jsonata_rs :: Agent Playbook

## Mission
Rebuild the full JSONata reference implementation in Rust while preserving behavioural parity with the upstream JavaScript project. Use the official JSONata conformance suite as the oracle, replacing the JS internals from the bottom up with native Rust components until the entire stack is Rust-driven.

## Repository Layout
- `src/jsonata/`: pristine copy of upstream JSONata (keep read-only, sync directly from the upstream repo).
- `src/jsonata-js-rust/`: hybrid runtime mirroring upstream layout but delegating built-ins to Rust via FFI.
- `src/jsonata-rust/`: home for the native Rust crate, tests, and future WASM build artifacts.
- `src/jsonata-js-rust/native/`: `napi-rs` bridge exposing Rust helpers to the hybrid JS runtime, exporting a `load_functions()` bridge object.
- `Dockerfile`: base image with Node.js 20, Rust toolchain, and pnpm preconfigured.
- `compose.yml`: Docker Compose service wiring the dev container with workspace mounts and shared cargo cache.

## Operating Principles
- **Parity first:** Every Rust substitution must satisfy the existing JSONata test corpus before proceeding upward in the stack.
- **Dual runtime:** Maintain a hybrid JS/Rust runtime so that we can progressively swap components without breaking the public Node.js API.
- **Observability:** Record benchmarks and compliance deltas at each replacement stage.
- **Regression guards:** Add Rust-side unit/property tests mirroring any upstream scenario that previously failed.
- **User first:** If user interrupted work and ask to do something different than you think is best - count it with priority, because probably user spotted some wrong behavior and want align current run.

## before dig deep into project, just run tests

## Current Focus
- napi bridge upgraded to `napi` 3.3 without compat shim; native addon builds again and mocha harness runs end-to-end (failing only on still-missing built-ins).
- Next sprint: finish higher-order/aggregate helpers so the remaining conformance buckets stop falling back to JS (`$map`, `$filter`, `$foldLeft`, `$single`, `$zip`, `$sift`, `$distinct`, `$merge`, etc.).
- Use the container test harness (fails allowed) to refresh the failure surface and drop logs for analysis:  
  `docker compose run --rm -e DOCKER_CONFIG=/workspace/.docker --workdir /workspace/src/jsonata-js-rust dev pnpm test || true`
- The test run writes its full transcript to `tmp/pnpm-test-last.log`; surface the highest-frequency gaps with:  
  `rg "Function '([^']+)'" -o --no-line-number tmp/pnpm-test-last.log | sort | uniq -c | sort -nr`
- Scan for bridge issues (current `$map/$zip` still yield `Error: JS: [object Object]`) via:  
  `rg -n "Error: JS" tmp/pnpm-test-last.log`
- Prioritise the counts from the latest log snapshot (higher first): `$single` parity (D3138/D3139 handling), `$foldLeft`/`$reduce` arity validation, async `$map/$zip` pipelines, and the regex map helpers.
## Execution Rules
- **Container-only automation:** Run builds, tests, and tooling exclusively through Docker Compose (`docker compose run --rm dev …`). Host-level execution of the toolchain is off-limits.
- **Source layout contract:** Treat `src/jsonata/` as upstream-read-only, implement Rust-native logic in `src/jsonata-rust/`, and maintain hybrid glue plus JavaScript-facing tweaks under `src/jsonata-js-rust/`.
- **Hybrid runtime flexibility:** Modify `src/jsonata-js-rust/` as needed to integrate Rust shims, adjust harness scripts, or skip/guard tests (for example, external HTTP probes) while documenting deviations from the upstream suite.
- **Deterministic tooling cache:** Keep the container’s Corepack/Cargo state under `/workspace/.corepack`, `/workspace/.home`, and `/workspace/.cargo` so repeated runs reuse downloaded toolchains; these paths are bind-mounted and ignored by git.
- **Docker config location:** When invoking tooling inside the container ensure `DOCKER_CONFIG` points at `/workspace/.docker` (host path `${REPO_ROOT}/.docker`) so Docker and Buildx reuse the mounted credentials/cache.
- **Container command conventions:** The dev container mounts the repo at `/workspace` but Node tooling lives under `src/jsonata-js-rust`; always set `--workdir /workspace/src/jsonata-js-rust` (or `bash -lc "cd /workspace/src/jsonata-js-rust && …"`) when running package scripts so `pnpm` can find `package.json`.

## Workflow Phases
1. **Bootstrap**
   - Keep `src/jsonata` in lockstep with the upstream repository (no edits).
   - Mirror the upstream tree into `src/jsonata-js-rust` where patches can be applied to call Rust shims without breaking layout.
   - Scaffold the Rust crate inside `src/jsonata-rust` exposing a C-compatible surface callable from Node via `napi-rs` (preferred) or `neon` fallback.
   - Build the dev container (`docker compose build`) to obtain a reproducible Node+Rust toolchain.
   - Baseline by running the upstream JS test suite unchanged inside the container (`docker compose run --rm --workdir /workspace/src/jsonata-js-rust dev pnpm test`).
   - Introduce the hybrid bridge in `src/jsonata-js-rust/native` and replace `functions.js` with a Rust-provided registry (non-port sections currently throw `not implemented`).
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
- `src/jsonata-js-rust/scripts/build-native.js`: compiles the `napi` bridge so JS tests can load Rust helpers.
- `src/jsonata-js-rust/scripts/skip-check-coverage.js`: placeholder disabling coverage gates while the bridge evolves.
- `src/jsonata-js-rust/scripts/skip-browser-build.js`: placeholder disabling browser bundling (Node runtime focus).
- `src/jsonata-js-rust/src/functions.js`: now a thin shim that imports `native.load_functions()`; Rust controls the full built-in surface.
- `cargo xtask coverage`: drives code coverage + regression summary.
- Tooling: `nyc` bumped to `17.1.0` (pulls in `foreground-child@3.3.1`) to avoid TTY/exit-code crashes when the native bridge aborts mid-test.
- Run `cargo outdated` for the N-API bridge via container: `docker compose run --rm -e DOCKER_CONFIG=/workspace/.docker --workdir /workspace/src/jsonata-js-rust/native dev bash -lc 'export PATH=/workspace/.cargo/bin:/opt/rust/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin && cargo outdated'`.
- codex shall run `docker compose` command with escalated permissions, it will fail otherwise due to environmant setup. 

## Status Tracking
- Maintain a `PORTING.md` matrix mapping JSONata features to Rust modules with state (`pending`, `partial`, `done`).
- Every PR must include: affected layer, tests run, parity proof.
- With the JS fallback removed, expect the upstream JS suite to fail until each built-in is reimplemented; track failing groups against the porting matrix. Current Rust surface covers math helpers together with `lookup`, `append`, `exists`, and `keys`.

## Open Questions
- Decide between `napi-rs` vs `neon` vs pure WASM bridge for the Node adapter.
- Determine the canonical async runtime (`tokio` vs `async-std`) before committing to APIs.
- Explore automatic differential testing against the JavaScript interpreter for fuzzed expressions.

## Current Action Items
- Extend the N-API bridge with a unified callable wrapper that can represent JSONata callbacks as Rust-owned functions (sync and async), allowing the JS suite for user-defined functions to run natively again.
- Re-enable the skipped user-defined-function tests once the wrapper lands to keep parity coverage for async evaluation paths.
- Finish porting the remaining higher-order helpers in Rust (`$map`, `$each`, `$number`, partial application helpers) so the hybrid runtime can clear the dependent conformance groups.

## N-API Migration Plan
- Rework the native bridge to the napi 3.3 API surface (no `compat-mode`), swapping direct `Js*` wrappers for `Unknown/Object/Function` helpers and rebuilding the threadsafe callback pipeline.
- Replace the ad-hoc conversions with helper utilities that return raw `sys::napi_value`, ensuring lifetimes line up with the new `FunctionCallContext`.
- Once the bridge compiles, rerun `docker compose run --rm --workdir /workspace/src/jsonata-js-rust dev pnpm test` to capture the updated failure surface before resuming feature work.

## Refactor Tips
- `docker compose run --rm --workdir /workspace/src/jsonata-js-rust dev pnpm test` — quickest end-to-end signal for napi regressions.
- `docker compose run --rm --workdir /workspace/src/jsonata-js-rust/native dev cargo check` — faster compile feedback for the bridge crate.
- `rg "napi_create" tmp/napi-rs/crates` — locate upstream napi-rs call patterns when mirroring bindings.
- `rg --files -g"*.rs" src/jsonata-js-rust/native` — scope the Rust bridge surface when hunting definitions.
- `docker compose run --rm --workdir /workspace/src/jsonata-js-rust/native dev bash -lc "RUST_LOG=napi=trace pnpm test"` — surface verbose napi diagnostics inside the container.

## Callback Bridging Design Notes
- Introduce a `JsonValue::Function` variant backed by an `Arc<dyn JsonCallable>` so Rust helpers can accept and invoke callback arguments without inspecting their origin.
- Define `JsonCallable` to return a boxed future that resolves to a `JsonValue`, allowing both synchronous values and promises/generators to be surfaced with the same API.
- When converting from JS to Rust, capture function-like values (`_jsonata_function`, `_jsonata_lambda`, plain JS functions, and generators) by creating a `ThreadsafeFunction` wrapper that marshals arguments/results and preserves the JSONata `focus` object as the invocation `this`.
- Extend the native bridge so every Rust entrypoint receives the `focus` handle (`ctx.this`) and can pass it through to callable wrappers, keeping environment-sensitive behaviour intact.
- Normalise callback argument preparation (mirroring `hofFuncArgs`, arity checks, generator unrolling) inside the wrapper so higher-order Rust helpers (`$map`, `$filter`, `$single`, etc.) can treat callbacks as opaque `JsonCallable`.
