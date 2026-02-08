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
- `src/jsonata-js-rust/` — hybrid facade that now sources every built-in from the Rust bridge.
- `src/jsonata-js-rust/native/` — `napi-rs` bridge exporting `load_functions()` (math helpers implemented, others stubbed for porting).
- `src/jsonata-rust/` — Rust crate workspace (crate scaffolding, tests, WASM targets).
- `Dockerfile` — base image providing Node.js 20, Rust toolchain, and pnpm.
- `compose.yml` — Docker Compose definition for the dev environment with Node + Rust.
- `src/jsonata-js-rust/scripts/` — helper scripts for compiling the bridge and (temporarily) skipping coverage/browser builds while we focus on parity.

## Getting Started
> Bootstrap is in progress; expect these steps to evolve as tooling lands.

1. Clone the repository:
   ```bash
   git clone https://github.com/<org>/async_jsonata_rs.git
   cd async_jsonata_rs
   ```
2. Build the container image:
   ```bash
   docker compose build
   ```
3. Use Docker Compose for all commands (host toolchain is not used).  
   The snippets below are the canonical command prefix:
   ```bash
   docker compose run --rm \
     -e DOCKER_CONFIG=/workspace/.docker \
     --workdir /workspace/src/jsonata-js-rust \
     dev bash -lc 'export PATH=/workspace/.cargo/bin:/opt/rust/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin; <COMMAND>'
   ```
4. Install JS dependencies:
   ```bash
   docker compose run --rm \
     -e DOCKER_CONFIG=/workspace/.docker \
     --workdir /workspace/src/jsonata-js-rust \
     dev bash -lc 'export PATH=/workspace/.cargo/bin:/opt/rust/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin; pnpm install'
   ```
5. Run the full hybrid suite (includes native build):
   ```bash
   docker compose run --rm \
     -e DOCKER_CONFIG=/workspace/.docker \
     --workdir /workspace/src/jsonata-js-rust \
     dev bash -lc 'export PATH=/workspace/.cargo/bin:/opt/rust/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin; pnpm test'
   ```
6. Fast feedback commands:
   - Native bridge compile check:
     ```bash
     docker compose run --rm \
       -e DOCKER_CONFIG=/workspace/.docker \
       --workdir /workspace/src/jsonata-js-rust/native \
       dev cargo check
     ```
   - Targeted parser tests (Rust parser path):
     ```bash
     docker compose run --rm \
       -e DOCKER_CONFIG=/workspace/.docker \
       --workdir /workspace/src/jsonata-js-rust \
       dev bash -lc './node_modules/.bin/mocha --require ./scripts/skip-user-defined-functions.js test/parser-recovery.js'
     ```
   - Targeted implementation tests:
     ```bash
     docker compose run --rm \
       -e DOCKER_CONFIG=/workspace/.docker \
       --workdir /workspace/src/jsonata-js-rust \
       dev bash -lc './node_modules/.bin/mocha --require ./scripts/skip-user-defined-functions.js test/implementation-tests.js'
     ```

## Contributing
1. Claim a feature in `PORTING.md` marked `pending` or `partial`.
2. Implement the behaviour in Rust under `src/jsonata-rust` with thorough unit tests.
3. Surface it to Node by patching `src/jsonata-js-rust` (FFI call) and ensure upstream JS tests still pass.
4. Mirror the added coverage with Rust integration tests and record results in the tracking matrix.

## License
`async_jsonata_rs` will follow the upstream JSONata licensing model (currently MIT). Final licensing details will be confirmed before the first release.

## Acknowledgements
- [JSONata](https://jsonata.org/) — the original project and specification.
- [napi-rs](https://napi.rs/) — for enabling seamless Rust <
→ Node.js interoperability.

- project source contins clone of JSONata repository maintained by [JSONata Limited](https://jsonata.org/).

