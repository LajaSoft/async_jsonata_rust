use std::collections::HashMap;
use std::sync::Arc;
use futures::future::BoxFuture;

use crate::functions::{core, math, strings};
use crate::types::{FunctionContext, JsonCallable, JsonError, JsonFunction, JsonValue};

/// Convert JsonValue to Option<f64> for math functions
fn json_value_to_number(value: &JsonValue) -> Option<f64> {
    match value {
        JsonValue::Number(n) => Some(*n),
        JsonValue::Bool(true) => Some(1.0),
        JsonValue::Bool(false) => Some(0.0),
        JsonValue::String(s) => s.parse().ok(),
        _ => None,
    }
}

/// Convert Option<f64> back to JsonValue
fn number_to_json_value(value: Option<f64>) -> JsonValue {
    match value {
        Some(n) => JsonValue::Number(n),
        None => JsonValue::Undefined,
    }
}

/// Direct callable for built-in Rust functions
#[derive(Clone)]
struct BuiltinCallable {
    name: &'static str,
    arity: Option<usize>,
    handler: fn(&[JsonValue]) -> Result<JsonValue, JsonError>,
    async_handler: Option<fn(FunctionContext, &[JsonValue]) -> BoxFuture<'static, Result<JsonValue, JsonError>>>,
}

impl BuiltinCallable {
    fn sync_fn(name: &'static str, arity: Option<usize>, handler: fn(&[JsonValue]) -> Result<JsonValue, JsonError>) -> Self {
        Self {
            name,
            arity,
            handler,
            async_handler: None,
        }
    }

    fn async_fn(
        name: &'static str, 
        arity: Option<usize>,
        handler: fn(FunctionContext, &[JsonValue]) -> BoxFuture<'static, Result<JsonValue, JsonError>>
    ) -> Self {
        Self {
            name,
            arity,
            handler: |_| Ok(JsonValue::Undefined), // placeholder
            async_handler: Some(handler),
        }
    }
}

impl JsonCallable for BuiltinCallable {
    fn call(
        &self,
        ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
        if let Some(async_handler) = self.async_handler {
            async_handler(ctx, &args)
        } else {
            let handler = self.handler;
            let result = handler(&args);
            Box::pin(async move { result })
        }
    }

    fn arity(&self) -> Option<usize> {
        self.arity
    }

    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}

/// Create a registry of all built-in functions
pub fn create_builtin_registry() -> HashMap<String, JsonFunction> {
    let mut registry = HashMap::new();

    // Math functions - wrapper to convert JsonValue to/from math types
    registry.insert("abs".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "abs", Some(1), |args| {
            let num = json_value_to_number(&args[0]);
            let result = math::abs(num);
            Ok(number_to_json_value(result))
        }
    ))));
    
    registry.insert("floor".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "floor", Some(1), |args| {
            let num = json_value_to_number(&args[0]);
            let result = math::floor(num);
            Ok(number_to_json_value(result))
        }
    ))));
    
    registry.insert("ceil".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "ceil", Some(1), |args| {
            let num = json_value_to_number(&args[0]);
            let result = math::ceil(num);
            Ok(number_to_json_value(result))
        }
    ))));
    
    registry.insert("round".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "round", Some(2), |args| {
            let num = json_value_to_number(&args[0]);
            let precision = json_value_to_number(&args.get(1).unwrap_or(&JsonValue::Number(0.0)));
            let result = math::round(num, precision);
            Ok(number_to_json_value(result))
        }
    ))));
    
    registry.insert("sqrt".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "sqrt", Some(1), |args| {
            let num = json_value_to_number(&args[0]);
            let result = math::sqrt(num)?;
            Ok(number_to_json_value(result))
        }
    ))));
    
    registry.insert("power".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "power", Some(2), |args| {
            let base = json_value_to_number(&args[0]);
            let exponent = json_value_to_number(&args[1]);
            let result = math::power(base, exponent)?;
            Ok(number_to_json_value(result))
        }
    ))));
    
    registry.insert("random".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "random", Some(0), |_args| {
            let result = math::random();
            Ok(JsonValue::Number(result))
        }
    ))));
    
    // Core functions (sync)
    registry.insert("exists".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "exists", Some(1), |args| Ok(core::exists(&args[0]))
    ))));
    
    registry.insert("boolean".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "boolean", Some(1), |args| Ok(core::boolean(&args[0]))
    ))));
    
    registry.insert("not".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "not", Some(1), |args| Ok(core::not(&args[0]))
    ))));
    
    registry.insert("type".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "type", Some(1), |args| Ok(core::type_of(&args[0]))
    ))));
    
    registry.insert("keys".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "keys", Some(1), |args| Ok(core::keys(&args[0]))
    ))));
    
    registry.insert("zip".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "zip", None, |args| Ok(core::zip(args))
    ))));
    
    registry.insert("append".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "append", Some(2), |args| Ok(core::append(&args[0], &args[1]))
    ))));
    
    // Core functions (async)
    registry.insert("single".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::async_fn(
        "single", Some(2), |ctx, args| {
            let array = args[0].clone();
            let predicate = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            Box::pin(core::single(ctx, array, predicate))
        }
    ))));
    
    registry.insert("map".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::async_fn(
        "map", Some(2), |ctx, args| {
            let array = args[0].clone();
            let func = args[1].clone();
            Box::pin(core::map(ctx, array, func))
        }
    ))));
    
    registry.insert("filter".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::async_fn(
        "filter", Some(2), |ctx, args| {
            let array = args[0].clone();
            let func = args[1].clone();
            Box::pin(core::filter(ctx, array, func))
        }
    ))));
    
    registry.insert("foldLeft".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::async_fn(
        "foldLeft", Some(3), |ctx, args| {
            let sequence = args[0].clone();
            let func = args[1].clone();
            let init = args.get(2).cloned().unwrap_or(JsonValue::Undefined);
            Box::pin(core::fold_left(ctx, sequence, func, init))
        }
    ))));
    
    // String functions
    registry.insert("string".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "string", Some(1), |args| {
            let pretty = args.get(1).cloned().unwrap_or(JsonValue::Bool(false));
            let is_pretty = matches!(pretty, JsonValue::Bool(true));
            strings::string(&args[0], is_pretty)
        }
    ))));
    
    registry.insert("length".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "length", Some(1), |args| strings::length(&args[0])
    ))));
    
    registry.insert("substring".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "substring", Some(3), |args| {
            let start = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            let length = args.get(2).cloned().unwrap_or(JsonValue::Undefined);
            strings::substring(&args[0], &start, &length)
        }
    ))));
    
    registry.insert("substringBefore".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "substringBefore", Some(2), |args| strings::substring_before(&args[0], &args[1])
    ))));
    
    registry.insert("substringAfter".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "substringAfter", Some(2), |args| strings::substring_after(&args[0], &args[1])
    ))));
    
    registry.insert("uppercase".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "uppercase", Some(1), |args| strings::uppercase(&args[0])
    ))));
    
    registry.insert("lowercase".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "lowercase", Some(1), |args| strings::lowercase(&args[0])
    ))));
    
    registry.insert("trim".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "trim", Some(1), |args| strings::trim(&args[0])
    ))));
    
    registry.insert("pad".to_string(), JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(
        "pad", Some(3), |args| {
            let width = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            let char_value = args.get(2).cloned().unwrap_or(JsonValue::Undefined);
            strings::pad(&args[0], &width, &char_value)
        }
    ))));

    registry
}

/// Helper to look up a built-in function by name
pub fn lookup_builtin(name: &str) -> Option<JsonFunction> {
    // TODO: This creates the registry every time, should be cached
    let registry = create_builtin_registry();
    registry.get(name).cloned()
}