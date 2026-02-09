use std::cmp::Ordering;
use std::collections::HashMap;

use serde_json::Value;

use crate::error::Error;
use crate::functions::core;
use crate::types::{JsonArray, JsonFunction, JsonObject, JsonValue};

use super::{eval, Bindings};

pub(super) fn eval_binary(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
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

    let left = eval(lhs, input, focus, functions, bindings)?;

    if op == "and" {
        if !is_truthy(&left) {
            return Ok(JsonValue::Bool(false));
        }
        let right = eval(rhs, input, focus, functions, bindings)?;
        return Ok(JsonValue::Bool(is_truthy(&right)));
    }

    if op == "or" {
        if is_truthy(&left) {
            return Ok(JsonValue::Bool(true));
        }
        let right = eval(rhs, input, focus, functions, bindings)?;
        return Ok(JsonValue::Bool(is_truthy(&right)));
    }

    let right = eval(rhs, input, focus, functions, bindings)?;

    match op {
        "+" => number_binop(&left, &right, |a, b| a + b),
        "-" => number_binop(&left, &right, |a, b| a - b),
        "*" => number_binop(&left, &right, |a, b| a * b),
        "/" => number_binop(&left, &right, |a, b| a / b),
        "%" => number_binop(&left, &right, |a, b| a % b),
        "&" => Ok(JsonValue::String(concat_strings(&left, &right))),
        ".." => range_op(&left, &right),
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
}

fn number_binop(
    left: &JsonValue,
    right: &JsonValue,
    op: fn(f64, f64) -> f64,
) -> Result<JsonValue, Error> {
    let Some(a) = to_number(left) else {
        return Ok(JsonValue::Undefined);
    };
    let Some(b) = to_number(right) else {
        return Ok(JsonValue::Undefined);
    };
    Ok(JsonValue::Number(op(a, b)))
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
    let Some(start) = to_number(left) else {
        return Ok(JsonValue::Undefined);
    };
    let Some(end) = to_number(right) else {
        return Ok(JsonValue::Undefined);
    };
    if start.fract() != 0.0 || end.fract() != 0.0 {
        return Ok(JsonValue::Undefined);
    }

    let start = start as i64;
    let end = end as i64;
    let step = if start <= end { 1 } else { -1 };
    let mut current = start;
    let mut out = Vec::new();
    loop {
        out.push(JsonValue::Number(current as f64));
        if current == end {
            break;
        }
        current += step;
    }
    Ok(JsonValue::Array(JsonArray::new(out, true, false)))
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
