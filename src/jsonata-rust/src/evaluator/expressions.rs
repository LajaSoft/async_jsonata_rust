use std::collections::HashMap;

use serde_json::Value;

use crate::error::Error;
use crate::types::{JsonArray, JsonFunction, JsonObject, JsonValue};

use super::lambda;
use super::ops::to_number;
use super::value::{materialize_value, object_keys_from_value, upsert_object_property};
use super::{eval, Bindings};

pub(super) fn eval_block(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let expressions = node
        .get("expressions")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2015", "Block node missing expressions"))?;

    let mut local_bindings = bindings.clone();
    let mut last = JsonValue::Undefined;
    for expr in expressions {
        if expr.get("type").and_then(Value::as_str) == Some("bind") {
            let (name, mut value) = eval_bind(expr, input, focus, functions, &local_bindings)?;
            if let JsonValue::Function(function) = &value {
                if let Some(rebound) = lambda::bind_recursive_name(function, &name) {
                    value = JsonValue::Function(rebound);
                }
            }
            local_bindings.insert(name.clone(), value.clone());
            local_bindings.insert(format!("${name}"), value.clone());
            last = value;
            continue;
        }
        last = eval(expr, input, focus, functions, &local_bindings)?;
    }

    Ok(last)
}

pub(super) fn eval_unary(
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

    if op == "[" {
        let expressions = node
            .get("expressions")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::new("E2016", "Array unary missing expressions"))?;
        let mut out = Vec::with_capacity(expressions.len());
        for expr in expressions {
            let value = eval(expr, input, focus, functions, bindings)?;
            match value {
                JsonValue::Array(array) if array.is_sequence => {
                    for element in array.elements {
                        out.push(element);
                    }
                }
                other => out.push(other),
            }
        }
        return Ok(JsonValue::Array(JsonArray::new(out, false, false)));
    }

    if op == "{" {
        let pairs = node
            .get("lhs")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::new("E2019", "Object unary missing lhs"))?;
        let mut object = JsonObject(Vec::new());
        for pair in pairs {
            let pair_values = pair
                .as_array()
                .ok_or_else(|| Error::new("E2020", "Object pair must be array"))?;
            if pair_values.len() != 2 {
                return Err(Error::new(
                    "E2021",
                    "Object pair must contain key and value",
                ));
            }

            let key_value = eval(&pair_values[0], input, focus, functions, bindings)?;
            let mut keys = object_keys_from_value(&key_value);
            if keys.is_empty() {
                if let Some(literal_key) = extract_object_literal_key(&pair_values[0]) {
                    keys.push(literal_key);
                }
            }
            if keys.is_empty() {
                continue;
            }
            let value = eval(&pair_values[1], input, focus, functions, bindings)?;
            let value = materialize_value(&value);
            for key in keys {
                upsert_object_property(&mut object, key, value.clone());
            }
        }
        return Ok(JsonValue::Object(object));
    }

    if op == "-" {
        let expr = node
            .get("expression")
            .ok_or_else(|| Error::new("E2017", "Unary minus missing expression"))?;
        let value = eval(expr, input, focus, functions, bindings)?;
        if let Some(num) = to_number(&value) {
            return Ok(JsonValue::Number(-num));
        }
        return Ok(JsonValue::Undefined);
    }

    Err(Error::new(
        "E2018",
        format!("Unsupported unary operator: {op}"),
    ))
}

pub(super) fn eval_bind(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<(String, JsonValue), Error> {
    let lhs = node
        .get("lhs")
        .ok_or_else(|| Error::new("E2022", "Bind node missing lhs"))?;
    let rhs = node
        .get("rhs")
        .ok_or_else(|| Error::new("E2023", "Bind node missing rhs"))?;

    let name = lhs
        .get("value")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::new("E2024", "Bind lhs must be variable"))?
        .trim_start_matches('$')
        .to_owned();

    if name.is_empty() {
        return Err(Error::new("E2025", "Bind variable name is empty"));
    }

    let value = eval(rhs, input, focus, functions, bindings)?;
    Ok((name, value))
}

pub(super) fn eval_condition(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let condition = node
        .get("condition")
        .ok_or_else(|| Error::new("E2027", "Condition node missing condition"))?;
    let then_branch = node
        .get("then")
        .ok_or_else(|| Error::new("E2028", "Condition node missing then"))?;

    let predicate = eval(condition, input, focus, functions, bindings)?;
    if super::ops::is_truthy(&predicate) {
        return eval(then_branch, input, focus, functions, bindings);
    }

    if let Some(else_branch) = node.get("else") {
        return eval(else_branch, input, focus, functions, bindings);
    }

    Ok(JsonValue::Undefined)
}

fn extract_object_literal_key(expr: &Value) -> Option<String> {
    if expr.get("type").and_then(Value::as_str) == Some("name") {
        return expr.get("value").and_then(Value::as_str).map(|text| text.to_owned());
    }
    if expr.get("type").and_then(Value::as_str) != Some("path") {
        return None;
    }
    let steps = expr.get("steps").and_then(Value::as_array)?;
    if steps.len() != 1 {
        return None;
    }
    let step = &steps[0];
    if step.get("type").and_then(Value::as_str) != Some("name") {
        return None;
    }
    step.get("value")
        .and_then(Value::as_str)
        .map(|text| text.to_owned())
}
