use std::collections::HashMap;
use std::sync::Arc;

use crate::functions::datetime;
use crate::types::{JsonError, JsonFunction, JsonValue};

use super::arg;
use super::callable::BuiltinCallable;

fn as_string(value: &JsonValue) -> Option<&str> {
    match value {
        JsonValue::String(s) => Some(s.as_str()),
        _ => None,
    }
}

fn as_number(value: &JsonValue) -> Option<f64> {
    match value {
        JsonValue::Number(n) => Some(*n),
        _ => None,
    }
}

pub(super) fn register(registry: &mut HashMap<String, JsonFunction>) {
    registry.insert(
        "formatInteger".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(2), |args| {
            let value = arg(args, 0);
            let Some(num) = as_number(&value) else {
                return Ok(JsonValue::Undefined);
            };
            let picture = match as_string(&arg(args, 1)) {
                Some(p) => p.to_string(),
                None => {
                    return Err(JsonError::new(
                        "T0410",
                        "Argument 2 of function formatInteger does not match function signature",
                    ))
                }
            };
            Ok(JsonValue::String(datetime::format_integer(num, &picture)?))
        }))),
    );

    registry.insert(
        "parseInteger".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(2), |args| {
            let value = arg(args, 0);
            let Some(text) = as_string(&value).map(|s| s.to_string()) else {
                return Ok(JsonValue::Undefined);
            };
            let picture = match as_string(&arg(args, 1)) {
                Some(p) => p.to_string(),
                None => {
                    return Err(JsonError::new(
                        "T0410",
                        "Argument 2 of function parseInteger does not match function signature",
                    ))
                }
            };
            Ok(JsonValue::Number(datetime::parse_integer(&text, &picture)?))
        }))),
    );

    registry.insert(
        "fromMillis".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(3), |args| {
            let value = arg(args, 0);
            let Some(num) = as_number(&value) else {
                return Ok(JsonValue::Undefined);
            };
            let picture_arg = arg(args, 1);
            let picture = as_string(&picture_arg).map(|s| s.to_string());
            let timezone_arg = arg(args, 2);
            let timezone = as_string(&timezone_arg).map(|s| s.to_string());
            Ok(JsonValue::String(datetime::from_millis(
                num,
                picture.as_deref(),
                timezone.as_deref(),
            )?))
        }))),
    );

    registry.insert(
        "toMillis".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(2), |args| {
            let value = arg(args, 0);
            let Some(timestamp) = as_string(&value).map(|s| s.to_string()) else {
                return Ok(JsonValue::Undefined);
            };
            let picture_arg = arg(args, 1);
            match as_string(&picture_arg) {
                None => Ok(JsonValue::Number(datetime::to_millis_iso(&timestamp)?)),
                Some(picture) => match datetime::parse_datetime(&timestamp, picture)? {
                    Some(m) => Ok(JsonValue::Number(m)),
                    None => Ok(JsonValue::Undefined),
                },
            }
        }))),
    );
}
