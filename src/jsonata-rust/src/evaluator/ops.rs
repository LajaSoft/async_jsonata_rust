use std::cmp::Ordering;
use std::collections::HashMap;

use futures::future::BoxFuture;
use serde_json::Value;

use crate::error::Error;
use crate::functions::core;
use crate::types::{JsonArray, JsonFunction, JsonObject, JsonValue};

use super::{eval, Bindings};

pub(super) fn eval_binary<'a>(
    node: &'a Value,
    input: &'a JsonValue,
    focus: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    Box::pin(async move {
    let op = node
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let lhs = node
        .get("lhs")
        .ok_or_else(|| Error::new("E2012", "Binary node missing lhs"))?;
    let rhs = node
        .get("rhs")
        .ok_or_else(|| Error::new("E2013", "Binary node missing rhs"))?;

    let left = eval(lhs, input, focus, functions, bindings).await?;

    if op == "and" {
        if !is_truthy(&left) {
            return Ok(JsonValue::Bool(false));
        }
        let right = eval(rhs, input, focus, functions, bindings).await?;
        return Ok(JsonValue::Bool(is_truthy(&right)));
    }

    if op == "or" {
        if is_truthy(&left) {
            return Ok(JsonValue::Bool(true));
        }
        let right = eval(rhs, input, focus, functions, bindings).await?;
        return Ok(JsonValue::Bool(is_truthy(&right)));
    }

    let right = eval(rhs, input, focus, functions, bindings).await?;

    match op {
        "+" => number_binop(&left, &right, "+", |a, b| a + b),
        "-" => number_binop(&left, &right, "-", |a, b| a - b),
        "*" => number_binop(&left, &right, "*", |a, b| a * b),
        "/" => number_binop(&left, &right, "/", |a, b| a / b),
        "%" => number_binop(&left, &right, "%", |a, b| a % b),
        "&" => Ok(JsonValue::String(concat_strings(&left, &right))),
        ".." => range_op(&left, &right),
        "in" => Ok(JsonValue::Bool(evaluate_includes(&left, &right))),
        "=" => Ok(JsonValue::Bool(values_equal(&left, &right))),
        "!=" => Ok(JsonValue::Bool(!values_equal(&left, &right))),
        ">" => compare_values(&left, &right, "T2010", |o| o.is_gt()),
        ">=" => compare_values(&left, &right, "T2010", |o| o.is_gt() || o.is_eq()),
        "<" => compare_values(&left, &right, "T2009", |o| o.is_lt()),
        "<=" => compare_values(&left, &right, "T2010", |o| o.is_lt() || o.is_eq()),
        "and" | "or" => Err(Error::new("E2014", "Logical operator dispatch failure")),
        _ => Err(Error::new(
            "E2014",
            format!("Unsupported binary operator: {op}"),
        )),
    }
    })
}

fn number_binop(
    left: &JsonValue,
    right: &JsonValue,
    token: &str,
    op: fn(f64, f64) -> f64,
) -> Result<JsonValue, Error> {
    let a = strict_numeric_operand(left, "T2001", token)?;
    let b = strict_numeric_operand(right, "T2002", token)?;
    if a.is_none() || b.is_none() {
        return Ok(JsonValue::Undefined);
    }
    let a = a.unwrap_or(0.0);
    let b = b.unwrap_or(0.0);
    Ok(JsonValue::Number(op(a, b)))
}

fn strict_numeric_operand(
    value: &JsonValue,
    code: &str,
    token: &str,
) -> Result<Option<f64>, Error> {
    match value {
        JsonValue::Undefined => Ok(None),
        JsonValue::Number(num) if num.is_finite() => Ok(Some(*num)),
        JsonValue::Number(num) => Err(Error::new(
            "D1001",
            format!("Number out of range;value:{num};token:{token}"),
        )),
        _ => Err(Error::new(
            code,
            format!("Numeric operand expected;token:{token}"),
        )),
    }
}

fn concat_strings(left: &JsonValue, right: &JsonValue) -> String {
    let mut out = String::new();
    out.push_str(&stringify_value(left));
    out.push_str(&stringify_value(right));
    out
}

fn stringify_value(value: &JsonValue) -> String {
    match value {
        JsonValue::Undefined => String::new(),
        JsonValue::Null => "null".to_owned(),
        JsonValue::Bool(flag) => flag.to_string(),
        JsonValue::Number(num) => {
            if num.fract() == 0.0 {
                (*num as i64).to_string()
            } else {
                num.to_string()
            }
        }
        JsonValue::String(text) => text.clone(),
        JsonValue::Array(array) => {
            let parts: Vec<String> = array.elements.iter().map(stringify_value).collect();
            format!("[{}]", parts.join(","))
        }
        JsonValue::Object(JsonObject(entries)) => {
            let mut parts = Vec::with_capacity(entries.len());
            for (key, entry) in entries {
                parts.push(format!("\"{}\":{}", key, stringify_value(entry)));
            }
            format!("{{{}}}", parts.join(","))
        }
        JsonValue::Function(_) => String::new(),
    }
}

fn range_op(left: &JsonValue, right: &JsonValue) -> Result<JsonValue, Error> {
    let start = strict_integer_operand(left, "T2003", "..")?;
    let end = strict_integer_operand(right, "T2004", "..")?;
    if start.is_none() || end.is_none() {
        return Ok(JsonValue::Undefined);
    }
    let start = start.unwrap_or(0);
    let end = end.unwrap_or(0);
    if start > end {
        return Ok(JsonValue::Undefined);
    }
    let size = end - start + 1;
    if size > 10_000_000 {
        return Err(Error::new("D2014", format!("Range too large;value:{size}")));
    }
    let mut current = start;
    let mut out = Vec::new();
    while current <= end {
        out.push(JsonValue::Number(current as f64));
        current += 1;
    }
    Ok(JsonValue::Array(JsonArray::new(out, true, false)))
}

fn strict_integer_operand(
    value: &JsonValue,
    code: &str,
    token: &str,
) -> Result<Option<i64>, Error> {
    match value {
        JsonValue::Undefined => Ok(None),
        JsonValue::Number(num) if num.is_finite() && num.fract() == 0.0 => Ok(Some(*num as i64)),
        JsonValue::Number(num) if num.is_finite() => Err(Error::new(
            code,
            format!("Integer operand expected;value:{num};token:{token}"),
        )),
        JsonValue::Number(num) => Err(Error::new(
            "D1001",
            format!("Number out of range;value:{num};token:{token}"),
        )),
        _ => Err(Error::new(
            code,
            format!("Integer operand expected;token:{token}"),
        )),
    }
}

pub(super) fn compare_sort_values(left: Option<&JsonValue>, right: Option<&JsonValue>) -> Ordering {
    match (left, right) {
        (Some(JsonValue::Number(a)), Some(JsonValue::Number(b))) => {
            a.partial_cmp(b).unwrap_or(Ordering::Equal)
        }
        (Some(JsonValue::String(a)), Some(JsonValue::String(b))) => a.cmp(b),
        (Some(JsonValue::Bool(a)), Some(JsonValue::Bool(b))) => a.cmp(b),
        (Some(JsonValue::Null), Some(JsonValue::Null)) => Ordering::Equal,
        (Some(JsonValue::Undefined), Some(JsonValue::Undefined)) => Ordering::Equal,
        (Some(_), Some(_)) => Ordering::Equal,
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
    }
}

fn compare_values(
    left: &JsonValue,
    right: &JsonValue,
    invalid_code: &'static str,
    accept: fn(Ordering) -> bool,
) -> Result<JsonValue, Error> {
    if left.is_undefined() || right.is_undefined() {
        if !left.is_undefined() && !is_order_comparable(left) {
            return Err(Error::new(
                invalid_code,
                "The comparison operator requires two numbers or two strings",
            ));
        }
        if !right.is_undefined() && !is_order_comparable(right) {
            return Err(Error::new(
                invalid_code,
                "The comparison operator requires two numbers or two strings",
            ));
        }
        return Ok(JsonValue::Undefined);
    }

    let ordering = match (left, right) {
        (JsonValue::String(lhs), JsonValue::String(rhs)) => lhs.cmp(rhs),
        (JsonValue::Number(lhs), JsonValue::Number(rhs)) => lhs
            .partial_cmp(rhs)
            .ok_or_else(|| Error::new(invalid_code, "Unable to compare NaN values"))?,
        _ => {
            return Err(Error::new(
                invalid_code,
                "The comparison operator requires two numbers or two strings",
            ));
        }
    };
    Ok(JsonValue::Bool(accept(ordering)))
}

/// Mirrors upstream `evaluateIncludesExpression`: `lhs in rhs`. If either side
/// is undefined the result is false; a non-array rhs is treated as a singleton
/// array; membership uses value equality.
fn evaluate_includes(lhs: &JsonValue, rhs: &JsonValue) -> bool {
    if lhs.is_undefined() || rhs.is_undefined() {
        return false;
    }
    match rhs {
        JsonValue::Array(array) => array.elements.iter().any(|item| values_equal(lhs, item)),
        other => values_equal(lhs, other),
    }
}

fn is_order_comparable(value: &JsonValue) -> bool {
    matches!(value, JsonValue::String(_) | JsonValue::Number(_))
}

pub(super) fn to_number(value: &JsonValue) -> Option<f64> {
    match value {
        JsonValue::Number(n) => Some(*n),
        JsonValue::Bool(true) => Some(1.0),
        JsonValue::Bool(false) => Some(0.0),
        JsonValue::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

pub(super) fn values_equal(left: &JsonValue, right: &JsonValue) -> bool {
    match (left, right) {
        (JsonValue::Undefined, JsonValue::Undefined) => false,
        (JsonValue::Null, JsonValue::Null) => true,
        (JsonValue::Bool(a), JsonValue::Bool(b)) => a == b,
        (JsonValue::Number(a), JsonValue::Number(b)) => a == b,
        (JsonValue::String(a), JsonValue::String(b)) => a == b,
        (JsonValue::Array(a), JsonValue::Array(b)) => {
            if a.elements.len() != b.elements.len() {
                return false;
            }
            a.elements
                .iter()
                .zip(b.elements.iter())
                .all(|(lhs, rhs)| values_equal(lhs, rhs))
        }
        (JsonValue::Object(JsonObject(a)), JsonValue::Object(JsonObject(b))) => {
            if a.len() != b.len() {
                return false;
            }
            for (key, left_value) in a {
                let Some((_, right_value)) = b.iter().find(|(other_key, _)| other_key == key)
                else {
                    return false;
                };
                if !values_equal(left_value, right_value) {
                    return false;
                }
            }
            true
        }
        _ => false,
    }
}

pub(super) fn is_truthy(value: &JsonValue) -> bool {
    matches!(core::boolean(value), JsonValue::Bool(true))
}

pub(super) fn to_sequence(value: &JsonValue) -> Vec<JsonValue> {
    match value {
        JsonValue::Undefined => Vec::new(),
        JsonValue::Array(array) => array.elements.clone(),
        other => vec![other.clone()],
    }
}

pub(super) fn from_sequence(items: Vec<JsonValue>) -> JsonValue {
    match items.len() {
        0 => JsonValue::Undefined,
        1 => items.into_iter().next().unwrap_or(JsonValue::Undefined),
        _ => JsonValue::Array(JsonArray::new(items, true, false)),
    }
}

pub(super) fn normalize_sequence(value: JsonValue) -> JsonValue {
    match value {
        JsonValue::Array(array) if array.is_sequence => match array.elements.len() {
            0 => JsonValue::Undefined,
            1 => array
                .elements
                .into_iter()
                .next()
                .unwrap_or(JsonValue::Undefined),
            _ => JsonValue::Array(array),
        },
        other => other,
    }
}
