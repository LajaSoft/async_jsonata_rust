# JsonataValue Migration Plan

## Target Function
- `core::append` (`src/jsonata-rust/src/functions/core.rs`)
- Currently works with legacy `JsonValue` types and relies on bridged conversions in `function_registry.rs`.

## Migration Steps
1. **Audit Current Usage**
   - Confirm where `append` is registered on the JS side (`function_registry.rs`).
   - Note helper utilities it relies on (array cloning, sequence metadata).
2. **Refactor Rust Implementation**
   - Implement a `JsonataValue`-based variant of `core::append`, reusing existing array semantics.
   - Ensure sequence/outer wrapper flags mirror existing behaviour.
   - Add unit tests (or adapt existing ones) to cover mixed scalar/array inputs.
3. **Update Call Sites**
   - Switch the native registry to invoke the `JsonataValue` variant directly.
   - Remove transitional conversion helper usage for `append`.
4. **Validation**
   - Rebuild the native bridge and run the conformance suite subset (`pnpm test`, focusing on append-related cases).
   - Document any follow-up actions (e.g. other functions sharing helpers).

## Completion Notes
- **2025-10-18**: Added `core::append_jsonata` and wired `function_registry` to call it, while keeping the legacy `JsonValue` implementation for compatibility. Need follow-up unit tests plus eventual removal of the legacy path once remaining call sites migrate.

---

## Target Function
- `$sum` (`src/jsonata-rust/src/functions/math.rs` + registration in `function_registry.rs`)
- Still relies on legacy `JsonValue` + manual Vec<f64> extraction.

## Migration Steps
1. **Audit Current Usage**
   - Identify every place `sum` is exposed (macro-based registration + `option_number_to_js` helper).
   - Note edge cases (undefined input, array vs scalar, sequence metadata).
2. **Refactor Rust Implementation**
   - Introduce a `JsonataValue`-native helper that:
     - Accepts any JSONata sequence/number/bool/string (mirroring legacy coercions).
     - Normalizes numeric output via `normalize_js_number`.
     - Returns `JsonataValue::Undefined` for empty/undefined inputs.
     - Emits a descriptive `JsonError` when encountering non-numeric input.
   - Reuse or extend helpers for `JsonataValue` → `f64` coercion shared with other math ops.
3. **Update Registry**
   - Replace the macro-generated `$sum` registration with a custom closure that:
     - Converts JS args to `JsonataValue`.
     - Calls the new Rust helper.
     - Maps `JsonataValue` back to JS via the standard converter.
     - Uses `json_error_to_napi` for error propagation.
   - Ensure subsequent math helpers still build.
4. **Validation**
   - Rebuild the native bridge.
   - Run focused expressions (`$sum([...])`, concatenation cases) to confirm rounding and types.
   - Execute `pnpm test` and capture remaining failures; update plan with findings.

## Completion Notes
- **2025-10-18**: Implemented `math::sum_jsonata` and registered it via the new bridge. Numeric coercion + normalization now happen on the Rust side; existing Vec<f64> helper remains only for legacy callers (to be removed once the other math functions migrate).
