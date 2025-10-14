# async_jsonata_rs

Rust-native reimplementation of the JSONata query language with full compatibility with the upstream JavaScript reference engine. The long-term goal is a production-grade crate that can serve both Rust applications and Node.js consumers while delivering better performance, stronger typing, and async-first execution.

## Roadmap
- Keep a pristine copy of upstream JSONata under `src/jsonata` as the behavioural oracle.
- Run the original JavaScript test suite to establish a baseline before each Rust substitution.
- Build a Rust crate that can be invoked from Node.js (`napi-rs` target) so pieces can be swapped in gradually.
- Replace internals bottom-up: scalar helpers → data model utilities → parser/evaluator.
- Mirror the JS acceptance suite with Rust integration tests once subsystems are ported.
- Deliver a Rust crate, a WASM build, and an npm package backed by the Rust core.

## Current Workspace Layout
- `src/jsonata/` — untouched upstream JSONata sources and tests (do not modify).
- `src/jsonata-js-rust/` — JS facade that will call into Rust replacements via FFI.
- `src/jsonata-rust/` — Rust crate workspace (crate scaffolding, tests, WASM targets).
- `Dockerfile` — base image providing Node.js 20, Rust toolchain, and pnpm.
- `compose.yml` — Docker Compose definition for the dev environment with Node + Rust.
- `scripts/` — automation helpers (planned; e.g. sync upstream, run conformance).

## Getting Started
> Bootstrap is in progress; expect these steps to evolve as tooling lands.

1. Clone the repository:
   ```bash
   git clone https://github.com/<org>/async_jsonata_rs.git
   cd async_jsonata_rs
   ```
2. Install the toolchain prerequisites:
   - Rust `stable` (via `rustup`)
   - Node.js 18+
   - `pnpm` (or `npm`) for managing the upstream JSONata dependencies
3. Validate the upstream baseline (from the host or the upcoming Docker image):
   ```bash
   cd src/jsonata
   pnpm install
   pnpm test
   ```
   The existing output defines the expected behaviour before any Rust substitution.
4. Use Docker Compose for a reproducible toolchain:
   ```bash
   docker compose build
   docker compose run --rm dev bash -lc "cd src/jsonata && pnpm install"
   docker compose run --rm dev bash -lc "cd src/jsonata && pnpm test"
   ```
   The container mounts the repository under `/workspace`, sharing cargo and pnpm caches across runs.

As Rust components become available, `cargo test` will provide fine-grained validation of the ported modules.

## Contributing
1. Claim a feature in `PORTING.md` marked `pending` or `partial`.
2. Implement the behaviour in Rust under `src/jsonata-rust` with thorough unit tests.
3. Surface it to Node by patching `src/jsonata-js-rust` (FFI call) and ensure upstream JS tests still pass.
4. Mirror the added coverage with Rust integration tests and record results in the tracking matrix.

## License
`async_jsonata_rs` will follow the upstream JSONata licensing model (currently MIT). Final licensing details will be confirmed before the first release.
