use std::collections::HashMap;

use crate::types::{JsonFunction, JsonValue};

mod callable;
mod common;
mod core;
mod datetime;
mod errors;
mod math;
mod regex;
mod strings;

/// Create a registry of all built-in functions.
///
/// # Examples
/// ```rust
/// let registry = async_jsonata_rust::registry::create_builtin_registry();
/// assert!(registry.contains_key("sqrt"));
/// ```
pub fn create_builtin_registry() -> HashMap<String, JsonFunction> {
    let mut registry = HashMap::new();

    math::register(&mut registry);
    core::register(&mut registry);
    datetime::register(&mut registry);
    strings::register(&mut registry);
    regex::register(&mut registry);
    errors::register(&mut registry);

    registry
}

/// Helper to look up a built-in function by name.
///
/// # Examples
/// ```rust
/// let sqrt = async_jsonata_rust::registry::lookup_builtin("sqrt");
/// assert!(sqrt.is_some());
/// ```
pub fn lookup_builtin(name: &str) -> Option<JsonFunction> {
    let registry = create_builtin_registry();
    registry.get(name).cloned()
}

fn arg(args: &[JsonValue], index: usize) -> JsonValue {
    args.get(index).cloned().unwrap_or(JsonValue::Undefined)
}
