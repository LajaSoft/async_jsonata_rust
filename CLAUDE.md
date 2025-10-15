# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust reimplementation of the JSONata query language with Node.js bindings via napi-rs. The goal is to achieve full compatibility with the upstream JavaScript reference engine while providing better performance and async-first execution.

## Repository Structure

- `src/jsonata/` - Pristine copy of upstream JSONata (read-only, do not modify)
- `src/jsonata-js-rust/` - Hybrid JS/Rust runtime that delegates built-ins to Rust via FFI
- `src/jsonata-rust/` - Pure Rust crate containing core JSONata implementation
- `src/jsonata-js-rust/native/` - N-API bridge exposing Rust functions to Node.js
- `tmp/` - Contains reference implementations and test artifacts

## Development Commands

All development must be done through Docker Compose - never run commands directly on the host:

### Building and Testing
```bash
# Build the development container
docker compose build

# Install dependencies in the hybrid runtime
docker compose run --rm --workdir /workspace/src/jsonata-js-rust dev pnpm install

# Run the JavaScript test suite (includes Rust bridge compilation)
docker compose run --rm --workdir /workspace/src/jsonata-js-rust dev pnpm test

# Build only the native bridge
docker compose run --rm --workdir /workspace/src/jsonata-js-rust dev pnpm run build:native

# Check Rust code compilation
docker compose run --rm --workdir /workspace/src/jsonata-js-rust/native dev cargo check

# Run cargo commands with proper environment
docker compose run --rm --workdir /workspace/src/jsonata-js-rust/native dev bash -lc 'export PATH=/workspace/.cargo/bin:/opt/rust/bin:$PATH && cargo test'
```

### Baseline Testing
```bash
# Test upstream JSONata (behavioral oracle)
docker compose run --rm --workdir /workspace/src/jsonata dev pnpm install
docker compose run --rm --workdir /workspace/src/jsonata dev pnpm test
```

### Development Analysis
```bash
# Run tests with failure tolerance and capture logs
docker compose run --rm -e DOCKER_CONFIG=/workspace/.docker --workdir /workspace/src/jsonata-js-rust dev pnpm test || true

# Analyze missing function frequency from test logs
docker compose run --rm dev bash -c "rg \"Function '([^']+)'\" -o --no-line-number /workspace/tmp/pnpm-test-last.log | sort | uniq -c | sort -nr"

# Find bridge errors in test logs
docker compose run --rm dev bash -c "rg -n \"Error: JS\" /workspace/tmp/pnpm-test-last.log"
```

## Architecture

### Technology Stack
- **Runtime**: Node.js 20 with Docker containerization
- **Rust Bridge**: napi-rs 3.3 (no compat mode)
- **Build System**: pnpm for Node.js dependencies, Cargo for Rust
- **Testing**: Mocha with nyc coverage for JS, cargo test for Rust

### Key Components

1. **N-API Bridge** (`src/jsonata-js-rust/native/`):
   - Exposes `load_functions()` to JavaScript
   - Handles JS ↔ Rust value conversion
   - Implements threadsafe function callbacks for async operations

2. **Rust Core** (`src/jsonata-rust/`):
   - Core JSONata functions organized by category (math, strings, core)
   - Type system supporting JSONata values, functions, and async operations
   - Currently implements: math helpers, `lookup`, `append`, `exists`, `keys`

3. **Hybrid Runtime** (`src/jsonata-js-rust/`):
   - Modified JSONata that delegates built-ins to Rust
   - Falls back to "not implemented" errors for unported functions
   - Maintains API compatibility with upstream JSONata

## Development Workflow

### Container Environment
- Always use `docker compose run --rm dev` to execute commands
- Working directory is mounted at `/workspace`
- Use `--workdir /workspace/src/jsonata-js-rust` for Node.js operations
- Set `DOCKER_CONFIG=/workspace/.docker` for Docker operations inside container

### Function Porting Process
1. Identify target function from upstream JSONata
2. Implement in Rust under `src/jsonata-rust/src/functions/`
3. Add to N-API bridge exports in `src/jsonata-js-rust/native/src/lib.rs`
4. Update function registry in `src/jsonata-js-rust/src/functions.js`
5. Run tests to verify behavioral parity

### Current Status
- NAPI bridge upgraded to 3.3 (no compat mode)
- Math functions, basic core functions implemented
- Higher-order functions (`$map`, `$filter`, etc.) still need porting
- Test failures expected until remaining built-ins are implemented

## Important Notes

- Test logs are automatically captured in `tmp/pnpm-test-last.log`
- Never run Python directly - always use Docker
- Preserve upstream JSONata in `src/jsonata/` unchanged
- Follow existing code patterns when implementing new functions
- All builds and tests must pass through the containerized environment