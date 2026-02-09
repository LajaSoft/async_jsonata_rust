use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::Arc;
use time::{OffsetDateTime, UtcOffset};

use crate::functions::{core, math, regex, strings};
use crate::types::{FunctionContext, JsonCallable, JsonError, JsonFunction, JsonObject, JsonValue};

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

fn parse_timezone_offset(value: &str) -> Option<UtcOffset> {
    if value.len() != 5 {
        return None;
    }
    let sign = match &value[0..1] {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };
    let hours: i8 = value[1..3].parse().ok()?;
    let minutes: i8 = value[3..5].parse().ok()?;
    let hours = hours.saturating_mul(sign);
    let minutes = minutes.saturating_mul(sign);
    UtcOffset::from_hms(hours, minutes, 0).ok()
}

fn format_now_iso_utc(now: OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        now.millisecond()
    )
}

fn format_now_custom(now: OffsetDateTime, timezone: Option<&str>) -> String {
    let offset = timezone
        .and_then(parse_timezone_offset)
        .unwrap_or(UtcOffset::UTC);
    let localized = now.to_offset(offset);
    let hour24 = localized.hour();
    let hour12 = match hour24 % 12 {
        0 => 12,
        value => value,
    };
    let meridiem = if hour24 < 12 { "am" } else { "pm" };
    let offset_seconds = offset.whole_seconds();
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    let total = offset_seconds.unsigned_abs();
    let tz_hours = total / 3600;
    let tz_minutes = (total % 3600) / 60;

    format!(
        "{}:{:02}{} GMT{}{:02}:{:02}",
        hour12,
        localized.minute(),
        meridiem,
        sign,
        tz_hours,
        tz_minutes
    )
}

fn sum_json_value(value: &JsonValue) -> Result<JsonValue, JsonError> {
    match value {
        JsonValue::Undefined | JsonValue::Null => Ok(JsonValue::Undefined),
        JsonValue::Array(array) => {
            if array.elements.is_empty() {
                return Ok(JsonValue::Undefined);
            }

            let mut total = 0.0;
            for element in &array.elements {
                let Some(number) = json_value_to_number(element) else {
                    return Err(JsonError::new(
                        "D3050",
                        "$sum() expects the input array to contain only numeric values",
                    ));
                };
                total += number;
            }
            Ok(JsonValue::Number(total))
        }
        other => {
            let Some(number) = json_value_to_number(other) else {
                return Err(JsonError::new(
                    "D3050",
                    "$sum() expects a numeric argument or an array of numerics",
                ));
            };
            Ok(JsonValue::Number(number))
        }
    }
}

fn clone_json_value(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Array(array) => JsonValue::Array(crate::types::JsonArray::new(
            array.elements.iter().map(clone_json_value).collect(),
            array.is_sequence,
            array.outer_wrapper,
        )),
        JsonValue::Object(JsonObject(entries)) => JsonValue::Object(JsonObject(
            entries
                .iter()
                .map(|(key, item)| (key.clone(), clone_json_value(item)))
                .collect(),
        )),
        other => other.clone(),
    }
}

/// Direct callable for built-in Rust functions
#[derive(Clone)]
struct BuiltinCallable {
    arity: Option<usize>,
    handler: fn(&[JsonValue]) -> Result<JsonValue, JsonError>,
    async_handler: Option<
        fn(FunctionContext, &[JsonValue]) -> BoxFuture<'static, Result<JsonValue, JsonError>>,
    >,
}

impl BuiltinCallable {
    fn sync_fn(
        arity: Option<usize>,
        handler: fn(&[JsonValue]) -> Result<JsonValue, JsonError>,
    ) -> Self {
        Self {
            arity,
            handler,
            async_handler: None,
        }
    }

    fn async_fn(
        arity: Option<usize>,
        handler: fn(
            FunctionContext,
            &[JsonValue],
        ) -> BoxFuture<'static, Result<JsonValue, JsonError>>,
    ) -> Self {
        Self {
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

/// Create a registry of all built-in functions.
///
/// # Examples
/// ```rust
/// let registry = async_jsonata_rust::registry::create_builtin_registry();
/// assert!(registry.contains_key("sqrt"));
/// ```
pub fn create_builtin_registry() -> HashMap<String, JsonFunction> {
    let mut registry = HashMap::new();

    // Math functions - wrapper to convert JsonValue to/from math types
    registry.insert(
        "abs".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let num = json_value_to_number(&input);
            let result = math::abs(num);
            Ok(number_to_json_value(result))
        }))),
    );

    registry.insert(
        "floor".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let num = json_value_to_number(&input);
            let result = math::floor(num);
            Ok(number_to_json_value(result))
        }))),
    );

    registry.insert(
        "ceil".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let num = json_value_to_number(&input);
            let result = math::ceil(num);
            Ok(number_to_json_value(result))
        }))),
    );

    registry.insert(
        "round".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(2), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let num = json_value_to_number(&input);
            let precision = json_value_to_number(&args.get(1).unwrap_or(&JsonValue::Number(0.0)));
            let result = math::round(num, precision);
            Ok(number_to_json_value(result))
        }))),
    );

    registry.insert(
        "sqrt".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let num = json_value_to_number(&input);
            let result = math::sqrt(num)?;
            Ok(number_to_json_value(result))
        }))),
    );

    registry.insert(
        "power".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(2), |args| {
            let lhs = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let rhs = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            let base = json_value_to_number(&lhs);
            let exponent = json_value_to_number(&rhs);
            let result = math::power(base, exponent)?;
            Ok(number_to_json_value(result))
        }))),
    );

    registry.insert(
        "random".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(0), |_args| {
            let result = math::random();
            Ok(JsonValue::Number(result))
        }))),
    );

    registry.insert(
        "sum".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let value = args.first().cloned().unwrap_or(JsonValue::Undefined);
            sum_json_value(&value)
        }))),
    );

    registry.insert(
        "count".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let value = args.first().cloned().unwrap_or(JsonValue::Undefined);
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
            let picture = args.first().and_then(|value| match value {
                JsonValue::String(text) => Some(text.as_str()),
                _ => None,
            });
            let timezone = args.get(1).and_then(|value| match value {
                JsonValue::String(text) => Some(text.as_str()),
                _ => None,
            });

            let formatted = if picture == Some("[h]:[M01][P] [z]") {
                format_now_custom(now, timezone)
            } else {
                format_now_iso_utc(now)
            };
            Ok(JsonValue::String(formatted))
        }))),
    );

    registry.insert(
        "clone".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            Ok(clone_json_value(&input))
        }))),
    );

    // Core functions (sync)
    registry.insert(
        "exists".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            Ok(core::exists(&input))
        }))),
    );

    registry.insert(
        "boolean".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            Ok(core::boolean(&input))
        }))),
    );

    registry.insert(
        "not".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            Ok(core::not(&input))
        }))),
    );

    registry.insert(
        "type".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            Ok(core::type_of(&input))
        }))),
    );

    registry.insert(
        "keys".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            Ok(core::keys(&input))
        }))),
    );

    registry.insert(
        "zip".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(None, |args| {
            Ok(core::zip(args))
        }))),
    );

    registry.insert(
        "append".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(2), |args| {
            let left = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let right = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            Ok(core::append(&left, &right))
        }))),
    );

    registry.insert(
        "lookup".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(2), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let key_value = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            let key = match key_value {
                JsonValue::String(text) => text,
                JsonValue::Number(num) => {
                    if num.fract() == 0.0 {
                        (num as i64).to_string()
                    } else {
                        num.to_string()
                    }
                }
                JsonValue::Bool(flag) => flag.to_string(),
                JsonValue::Null => "null".to_owned(),
                JsonValue::Undefined => return Ok(JsonValue::Undefined),
                _ => return Ok(JsonValue::Undefined),
            };
            Ok(core::lookup(&input, &key))
        }))),
    );

    // Core functions (async)
    registry.insert(
        "single".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(2), |ctx, args| {
            let array = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let predicate = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            Box::pin(core::single(ctx, array, predicate))
        }))),
    );

    registry.insert(
        "map".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(2), |ctx, args| {
            let array = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let func = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            Box::pin(core::map(ctx, array, func))
        }))),
    );

    registry.insert(
        "filter".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(2), |ctx, args| {
            let array = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let func = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            Box::pin(core::filter(ctx, array, func))
        }))),
    );

    registry.insert(
        "each".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(2), |ctx, args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let func = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            Box::pin(core::each(ctx, input, func))
        }))),
    );

    registry.insert(
        "sift".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(2), |ctx, args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let func = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            Box::pin(core::sift(ctx, input, func))
        }))),
    );

    registry.insert(
        "foldLeft".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(3), |ctx, args| {
            let sequence = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let func = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            let init = args.get(2).cloned().unwrap_or(JsonValue::Undefined);
            Box::pin(core::fold_left(ctx, sequence, func, init))
        }))),
    );

    // String functions
    registry.insert(
        "string".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let pretty = args.get(1).cloned().unwrap_or(JsonValue::Bool(false));
            let is_pretty = matches!(pretty, JsonValue::Bool(true));
            strings::string(&input, is_pretty)
        }))),
    );

    registry.insert(
        "length".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            strings::length(&input)
        }))),
    );

    registry.insert(
        "substring".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(3), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let start = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            let length = args.get(2).cloned().unwrap_or(JsonValue::Undefined);
            strings::substring(&input, &start, &length)
        }))),
    );

    registry.insert(
        "substringBefore".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(2), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let token = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            strings::substring_before(&input, &token)
        }))),
    );

    registry.insert(
        "substringAfter".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(2), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let token = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            strings::substring_after(&input, &token)
        }))),
    );

    registry.insert(
        "uppercase".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            strings::uppercase(&input)
        }))),
    );

    registry.insert(
        "lowercase".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            strings::lowercase(&input)
        }))),
    );

    registry.insert(
        "trim".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            strings::trim(&input)
        }))),
    );

    registry.insert(
        "pad".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(3), |args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let width = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            let char_value = args.get(2).cloned().unwrap_or(JsonValue::Undefined);
            strings::pad(&input, &width, &char_value)
        }))),
    );

    registry.insert(
        "contains".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(2), |ctx, args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let token = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            Box::pin(regex::contains_function(ctx, input, token))
        }))),
    );

    registry.insert(
        "match".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(3), |ctx, args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let matcher = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            let limit = args.get(2).cloned().unwrap_or(JsonValue::Undefined);
            Box::pin(regex::match_function(ctx, input, matcher, limit))
        }))),
    );

    registry.insert(
        "split".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(3), |ctx, args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let separator = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            let limit = args.get(2).cloned().unwrap_or(JsonValue::Undefined);
            Box::pin(regex::split_function(ctx, input, separator, limit))
        }))),
    );

    registry.insert(
        "replace".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(4), |ctx, args| {
            let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let pattern = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            let replacement = args.get(2).cloned().unwrap_or(JsonValue::Undefined);
            let limit = args.get(3).cloned().unwrap_or(JsonValue::Undefined);
            Box::pin(regex::replace_function(
                ctx,
                input,
                pattern,
                replacement,
                limit,
            ))
        }))),
    );

    registry.insert(
        "join".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(2), |args| {
            let values = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let separator = args.get(1).cloned().unwrap_or(JsonValue::Undefined);
            regex::join_function(values, separator)
        }))),
    );

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
    // TODO: This creates the registry every time, should be cached
    let registry = create_builtin_registry();
    registry.get(name).cloned()
}
