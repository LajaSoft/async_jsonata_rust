//! Error/assertion built-in functions: `$error` and `$assert`.
//!
//! These mirror the upstream JSONata reference implementation:
//! - `$error(message)` (`<s?:x>`) always throws `D3137`.
//! - `$assert(condition, message)` (`<bs?:x>`) throws `D3141` when the
//!   condition is false and returns nothing otherwise.
//!
//! Argument type mismatches surface as `T0410` to match the reference engine's
//! signature validation.

use crate::types::{JsonError, JsonValue};

/// `$error(message)` — always raises a `D3137` error.
///
/// The optional `message` argument must be a string; `undefined` (e.g. a
/// reference to a missing field, or no argument at all) falls back to the
/// default message. Any other type is a signature mismatch (`T0410`).
pub fn error(args: &[JsonValue]) -> Result<JsonValue, JsonError> {
    let message = match args.first() {
        None | Some(JsonValue::Undefined) => None,
        Some(JsonValue::String(text)) => Some(text.clone()),
        Some(_) => {
            return Err(JsonError::new(
                "T0410",
                "Argument 1 of function error does not match function signature",
            ));
        }
    };
    Err(JsonError::new(
        "D3137",
        message.unwrap_or_else(|| "$error() function evaluated".to_owned()),
    ))
}

/// `$assert(condition, message)` — raises `D3141` when `condition` is false.
///
/// The condition must be a boolean (`T0410` otherwise) and the optional message
/// must be a string. When the condition holds, the function returns nothing
/// (`undefined`).
pub fn assert(args: &[JsonValue]) -> Result<JsonValue, JsonError> {
    let condition = match args.first() {
        Some(JsonValue::Bool(flag)) => *flag,
        None | Some(JsonValue::Undefined) => false,
        Some(_) => {
            return Err(JsonError::new(
                "T0410",
                "Argument 1 of function assert does not match function signature",
            ));
        }
    };

    if condition {
        return Ok(JsonValue::Undefined);
    }

    let message = match args.get(1) {
        None | Some(JsonValue::Undefined) => None,
        Some(JsonValue::String(text)) => Some(text.clone()),
        Some(_) => {
            return Err(JsonError::new(
                "T0410",
                "Argument 2 of function assert does not match function signature",
            ));
        }
    };

    Err(JsonError::new(
        "D3141",
        message.unwrap_or_else(|| "$assert() statement failed".to_owned()),
    ))
}
