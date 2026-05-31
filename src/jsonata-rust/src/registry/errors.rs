use std::collections::HashMap;
use std::sync::Arc;

use crate::functions::errors;
use crate::types::JsonFunction;

use super::callable::BuiltinCallable;

pub(super) fn register(registry: &mut HashMap<String, JsonFunction>) {
    // Arity `Some(0)` prevents the evaluator from injecting the focus value as
    // an implicit first argument: `$error()`/`$assert()` carry no context
    // modifier in their upstream signatures (`<s?:x>` / `<bs?:x>`).
    registry.insert(
        "error".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(0), |args| {
            errors::error(args)
        }))),
    );

    registry.insert(
        "assert".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(0), |args| {
            errors::assert(args)
        }))),
    );
}
