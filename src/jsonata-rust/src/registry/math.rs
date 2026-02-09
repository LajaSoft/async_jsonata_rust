use std::collections::HashMap;
use std::sync::Arc;

use time::OffsetDateTime;

use crate::functions::math;
use crate::types::{JsonFunction, JsonValue};

use super::callable::BuiltinCallable;
use super::common::{
    clone_json_value, format_now_custom, format_now_iso_utc, json_value_to_number,
    number_to_json_value, sum_json_value,
};
use super::arg;

pub(super) fn register(registry: &mut HashMap<String, JsonFunction>) {
    registry.insert(
        "abs".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let num = json_value_to_number(&arg(args, 0));
            Ok(number_to_json_value(math::abs(num)))
        }))),
    );

    registry.insert(
        "floor".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let num = json_value_to_number(&arg(args, 0));
            Ok(number_to_json_value(math::floor(num)))
        }))),
    );

    registry.insert(
        "ceil".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let num = json_value_to_number(&arg(args, 0));
            Ok(number_to_json_value(math::ceil(num)))
        }))),
    );

    registry.insert(
        "round".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(2), |args| {
            let num = json_value_to_number(&arg(args, 0));
            let precision = json_value_to_number(&arg(args, 1).clone());
            Ok(number_to_json_value(math::round(num, precision)))
        }))),
    );

    registry.insert(
        "sqrt".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let num = json_value_to_number(&arg(args, 0));
            Ok(number_to_json_value(math::sqrt(num)?))
        }))),
    );

    registry.insert(
        "power".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(2), |args| {
            let base = json_value_to_number(&arg(args, 0));
            let exponent = json_value_to_number(&arg(args, 1));
            Ok(number_to_json_value(math::power(base, exponent)?))
        }))),
    );

    registry.insert(
        "random".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(0), |_args| {
            Ok(JsonValue::Number(math::random()))
        }))),
    );

    registry.insert(
        "sum".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let value = arg(args, 0);
            sum_json_value(&value)
        }))),
    );

    registry.insert(
        "count".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let value = arg(args, 0);
            Ok(math::count_value(&value))
        }))),
    );

    registry.insert(
        "millis".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(0), |_args| {
            Ok(JsonValue::Number(
                OffsetDateTime::now_utc().unix_timestamp_nanos() as f64 / 1_000_000.0,
            ))
        }))),
    );

    registry.insert(
        "now".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(2), |args| {
            let now = OffsetDateTime::now_utc();
            let picture = match arg(args, 0) {
                JsonValue::String(text) => Some(text),
                _ => None,
            };
            let timezone = match arg(args, 1) {
                JsonValue::String(text) => Some(text),
                _ => None,
            };

            let formatted = if picture.as_deref() == Some("[h]:[M01][P] [z]") {
                format_now_custom(now, timezone.as_deref())
            } else {
                format_now_iso_utc(now)
            };
            Ok(JsonValue::String(formatted))
        }))),
    );

    registry.insert(
        "clone".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let input = arg(args, 0);
            Ok(clone_json_value(&input))
        }))),
    );

    registry.insert(
        "number".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            math::number(&arg(args, 0))
        }))),
    );
}
