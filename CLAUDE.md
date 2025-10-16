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

### Major Architecture Change (2024)
**BREAKING CHANGE**: Replaced the flawed JSON serialization approach with a proper native type system:

- **Old Problem**: Tried to serialize JS functions/objects as JSON, causing "unexpected response" errors and cyclical call issues
- **New Solution**: `JsonataValue` enum with `NativeRef` variant that holds napi references to JS objects
- **Key Insight**: JSONata works with dict-like structures and expressions, NOT JSON. JSON conversion only when actually needed (stringify functions)
- **Result**: Test success rate jumped from ~40% to 98% (1698 passing / 34 failing)

### Technology Stack
- **Runtime**: Node.js 20 with Docker containerization
- **Rust Bridge**: napi-rs 3.3 (no compat mode)
- **Build System**: pnpm for Node.js dependencies, Cargo for Rust
- **Testing**: Mocha with nyc coverage for JS, cargo test for Rust
- **Type System**: Native `JsonataValue` with JS object references via `NativeRef`

### Key Components

1. **N-API Bridge** (`src/jsonata-js-rust/native/`):
   - Exposes `load_functions()` to JavaScript
   - Handles JS ↔ Rust value conversion
   - Implements threadsafe function callbacks for async operations

2. **Conversion System** (`src/jsonata-js-rust/native/src/conversion.rs`):
   - `js_to_jsonata_value()`: Converts JS values to native `JsonataValue`
   - `jsonata_value_to_js()`: Converts `JsonataValue` back to JS
   - Handles `NativeRef` for preserving JS function/object references
   - Backward compatibility converters for gradual migration

3. **Function Registry** (`src/jsonata-js-rust/native/src/function_registry.rs`):
   - Macro-based function registration system
   - Eliminates hundreds of lines of repetitive code
   - Categories: math functions, core functions, string functions
   - Auto-generates conversion wrappers for each function type

4. **Rust Core** (`src/jsonata-rust/`):
   - Core JSONata functions organized by category (math, strings, core)
   - Type system supporting JSONata values, functions, and async operations
   - Currently implements: math helpers, `lookup`, `append`, `exists`, `keys`

5. **Hybrid Runtime** (`src/jsonata-js-rust/`):
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
- ✅ **MAJOR BREAKTHROUGH**: Native type system implemented and working
- ✅ **Architecture Fixed**: Replaced broken JSON approach with `JsonataValue` + `NativeRef`
- ✅ **Test Results**: Dramatically improved to 1698 passing / 34 failing (was ~40% success)  
- ✅ **Code Quality**: Macro-based function registry eliminates repetitive code
- ✅ **Conversion System**: Working JS ↔ `JsonataValue` converters
- 🔄 **In Progress**: Migrating remaining functions to new architecture
- 🔄 **Remaining**: Fix NativeRef lifetime issues, complete higher-order functions

## Important Notes

- **Architecture Change**: Use `JsonataValue` for all new functions, not `JsonValue`
- **Type Conversion**: Use conversion functions in `conversion.rs` for JS ↔ Rust
- **Function Registration**: Add new functions via macros in `function_registry.rs`
- Test logs are automatically captured in `tmp/pnpm-test-last.log`
- Never run Python directly - always use Docker
- Preserve upstream JSONata in `src/jsonata/` unchanged
- All builds and tests must pass through the containerized environment

## Next Steps for Continuation

1. **Fix NativeRef Lifetime Issues**: Complete the function reference system
2. **Migrate Remaining Functions**: Move higher-order functions to `JsonataValue`
3. **Test Cyclical Calls**: Verify Rust→JS→Rust calls work with new system
4. **Performance Testing**: Benchmark the new native type system vs old approach