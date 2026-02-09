use std::collections::HashMap;

use futures::executor::block_on;
use serde_json::Value;

use crate::error::Error;
use crate::functions::core;
use crate::types::{FunctionContext, JsonArray, JsonFunction, JsonObject, JsonValue};

pub(crate) fn evaluate_expression(
    ast: &Value,
    input: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
) -> Result<JsonValue, Error> {
    eval(ast, input, input, functions)
}

fn eval(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
) -> Result<JsonValue, Error> {
    let node_type = node.get("type").and_then(Value::as_str).unwrap_or_default();

    match node_type {
        "path" => eval_path(node, input, focus, functions),
        "name" => {
            let name = node
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Ok(core::lookup(focus, name))
        }
        "variable" => eval_variable(node, input, functions),
        "string" => Ok(JsonValue::String(
            node.get("value")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        )),
        "number" => Ok(JsonValue::Number(
            node.get("value").and_then(Value::as_f64).unwrap_or(0.0),
        )),
        "value" => Ok(json_value_from_serde(node.get("value").unwrap_or(&Value::Null))),
        "function" => eval_function(node, input, focus, functions),
        "binary" => eval_binary(node, input, focus, functions),
        "apply" => eval_apply(node, input, focus, functions),
        "block" => eval_block(node, input, focus, functions),
        "unary" => eval_unary(node, input, focus, functions),
        _ => Err(Error::new(
            "E2001",
            format!("Unsupported AST node type: {node_type}"),
        )),
    }
}

fn eval_path(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
) -> Result<JsonValue, Error> {
    let mut current = focus.clone();
    let steps = node
        .get("steps")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2002", "Path node missing steps"))?;

    for step in steps {
        current = eval_path_step(step, input, &current, functions)?;
    }

    Ok(current)
}

fn eval_path_step(
    step: &Value,
    input: &JsonValue,
    current: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
) -> Result<JsonValue, Error> {
    let step_type = step.get("type").and_then(Value::as_str).unwrap_or_default();

    let mut out = match step_type {
        "name" => {
            let key = step
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default();
            core::lookup(current, key)
        }
        "function" => eval_function(step, input, current, functions)?,
        "variable" => eval_variable(step, input, functions)?,
        "number" => JsonValue::Number(step.get("value").and_then(Value::as_f64).unwrap_or(0.0)),
        "string" => JsonValue::String(
            step.get("value")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        ),
        other => {
            return Err(Error::new(
                "E2003",
                format!("Unsupported path step type: {other}"),
            ))
        }
    };

    if let Some(index) = step.get("index") {
        out = apply_index(&out, index);
    }

    if let Some(stages) = step.get("stages").and_then(Value::as_array) {
        for stage in stages {
            out = apply_stage(stage, input, &out, functions)?;
        }
    }

    Ok(out)
}

fn apply_stage(
    stage: &Value,
    input: &JsonValue,
    current: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
) -> Result<JsonValue, Error> {
    let stage_type = stage
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();

    match stage_type {
        "filter" => {
            let expr = stage
                .get("expr")
                .ok_or_else(|| Error::new("E2004", "Filter stage missing expr"))?;

            if expr.get("type").and_then(Value::as_str) == Some("number") {
                return Ok(apply_index(current, expr.get("value").unwrap_or(&Value::Null)));
            }

            let mut kept = Vec::new();
            for item in to_sequence(current) {
                let predicate = eval(expr, input, &item, functions)?;
                if is_truthy(&predicate) {
                    kept.push(item);
                }
            }
            Ok(from_sequence(kept))
        }
        "index" => {
            let index = stage.get("value").unwrap_or(&Value::Null);
            Ok(apply_index(current, index))
        }
        other => Err(Error::new(
            "E2005",
            format!("Unsupported stage type: {other}"),
        )),
    }
}

fn apply_index(current: &JsonValue, index: &Value) -> JsonValue {
    let Some(idx) = index.as_i64() else {
        return JsonValue::Undefined;
    };

    let seq = to_sequence(current);
    if seq.is_empty() {
        return JsonValue::Undefined;
    }

    let position = if idx < 0 {
        let from_end = seq.len() as i64 + idx;
        if from_end < 0 {
            return JsonValue::Undefined;
        }
        from_end as usize
    } else {
        idx as usize
    };

    seq.get(position).cloned().unwrap_or(JsonValue::Undefined)
}

fn eval_variable(
    node: &Value,
    input: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
) -> Result<JsonValue, Error> {
    let raw = node
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_default();

    if raw == "$" {
        return Ok(input.clone());
    }

    let name = raw.trim_start_matches('$');
    if let Some(func) = functions.get(name) {
        return Ok(JsonValue::Function(func.clone()));
    }

    Ok(JsonValue::Undefined)
}

fn eval_function(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
) -> Result<JsonValue, Error> {
    let procedure = node
        .get("procedure")
        .ok_or_else(|| Error::new("E2006", "Function node missing procedure"))?;

    let callable = resolve_callable(procedure, input, focus, functions)?;
    let arguments = node
        .get("arguments")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2007", "Function node missing arguments"))?;

    let mut args = Vec::with_capacity(arguments.len());
    for arg in arguments {
        args.push(eval(arg, input, focus, functions)?);
    }

    let ctx = FunctionContext::with_focus(crate::types::JsonataFocus::new(focus.clone()));
    block_on(callable.call(ctx, args)).map_err(Error::from)
}

fn resolve_callable(
    procedure: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
) -> Result<JsonFunction, Error> {
    let procedure_type = procedure
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();

    match procedure_type {
        "variable" => {
            let name = procedure
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim_start_matches('$')
                .to_owned();
            functions
                .get(name.as_str())
                .cloned()
                .ok_or_else(|| Error::new("T1006", format!("Unknown function: {name}")))
        }
        "path" => {
            let steps = procedure
                .get("steps")
                .and_then(Value::as_array)
                .ok_or_else(|| Error::new("E2008", "Procedure path missing steps"))?;
            if steps.len() != 1 {
                return Err(Error::new("T1006", "Function procedure path must be a single name"));
            }

            let step = &steps[0];
            let step_type = step.get("type").and_then(Value::as_str).unwrap_or_default();
            let name = match step_type {
                "name" | "variable" => step
                    .get("value")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim_start_matches('$')
                    .to_owned(),
                _ => {
                    return Err(Error::new(
                        "T1006",
                        format!("Unsupported function procedure step: {step_type}"),
                    ))
                }
            };

            functions
                .get(name.as_str())
                .cloned()
                .ok_or_else(|| Error::new("T1006", format!("Unknown function: {name}")))
        }
        _ => {
            let value = eval(procedure, input, focus, functions)?;
            match value {
                JsonValue::Function(func) => Ok(func),
                _ => Err(Error::new("T1006", "Procedure is not callable")),
            }
        }
    }
}

fn eval_apply(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
) -> Result<JsonValue, Error> {
    let lhs = node
        .get("lhs")
        .ok_or_else(|| Error::new("E2009", "Apply node missing lhs"))?;
    let rhs = node
        .get("rhs")
        .ok_or_else(|| Error::new("E2010", "Apply node missing rhs"))?;

    let base = eval(lhs, input, focus, functions)?;

    if rhs.get("type").and_then(Value::as_str) == Some("function") {
        let procedure = rhs
            .get("procedure")
            .ok_or_else(|| Error::new("E2011", "Apply function missing procedure"))?;
        let callable = resolve_callable(procedure, input, &base, functions)?;

        let mut args = vec![base.clone()];
        if let Some(extra_args) = rhs.get("arguments").and_then(Value::as_array) {
            for arg in extra_args {
                args.push(eval(arg, input, focus, functions)?);
            }
        }

        let ctx = FunctionContext::with_focus(crate::types::JsonataFocus::new(base));
        return block_on(callable.call(ctx, args)).map_err(Error::from);
    }

    let candidate = eval(rhs, input, &base, functions)?;
    match candidate {
        JsonValue::Function(callable) => {
            let ctx = FunctionContext::with_focus(crate::types::JsonataFocus::new(base.clone()));
            block_on(callable.call(ctx, vec![base])).map_err(Error::from)
        }
        _ => Err(Error::new("T1006", "Right side of apply is not callable")),
    }
}

fn eval_binary(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
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

    let left = eval(lhs, input, focus, functions)?;
    let right = eval(rhs, input, focus, functions)?;

    match op {
        "+" => number_binop(&left, &right, |a, b| a + b),
        "-" => number_binop(&left, &right, |a, b| a - b),
        "*" => number_binop(&left, &right, |a, b| a * b),
        "/" => number_binop(&left, &right, |a, b| a / b),
        "=" => Ok(JsonValue::Bool(values_equal(&left, &right))),
        "!=" => Ok(JsonValue::Bool(!values_equal(&left, &right))),
        ">" => number_cmp(&left, &right, |a, b| a > b),
        ">=" => number_cmp(&left, &right, |a, b| a >= b),
        "<" => number_cmp(&left, &right, |a, b| a < b),
        "<=" => number_cmp(&left, &right, |a, b| a <= b),
        "and" => Ok(JsonValue::Bool(is_truthy(&left) && is_truthy(&right))),
        "or" => Ok(JsonValue::Bool(is_truthy(&left) || is_truthy(&right))),
        _ => Err(Error::new(
            "E2014",
            format!("Unsupported binary operator: {op}"),
        )),
    }
}

fn eval_block(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
) -> Result<JsonValue, Error> {
    let expressions = node
        .get("expressions")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2015", "Block node missing expressions"))?;

    let mut last = JsonValue::Undefined;
    for expr in expressions {
        last = eval(expr, input, focus, functions)?;
    }

    Ok(last)
}

fn eval_unary(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
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
            out.push(eval(expr, input, focus, functions)?);
        }
        return Ok(JsonValue::Array(JsonArray::new(out, false, false)));
    }

    if op == "-" {
        let expr = node
            .get("expression")
            .ok_or_else(|| Error::new("E2017", "Unary minus missing expression"))?;
        let value = eval(expr, input, focus, functions)?;
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

fn number_binop(left: &JsonValue, right: &JsonValue, op: fn(f64, f64) -> f64) -> Result<JsonValue, Error> {
    let Some(a) = to_number(left) else {
        return Ok(JsonValue::Undefined);
    };
    let Some(b) = to_number(right) else {
        return Ok(JsonValue::Undefined);
    };
    Ok(JsonValue::Number(op(a, b)))
}

fn number_cmp(left: &JsonValue, right: &JsonValue, cmp: fn(f64, f64) -> bool) -> Result<JsonValue, Error> {
    let Some(a) = to_number(left) else {
        return Ok(JsonValue::Undefined);
    };
    let Some(b) = to_number(right) else {
        return Ok(JsonValue::Undefined);
    };
    Ok(JsonValue::Bool(cmp(a, b)))
}

fn to_number(value: &JsonValue) -> Option<f64> {
    match value {
        JsonValue::Number(n) => Some(*n),
        JsonValue::Bool(true) => Some(1.0),
        JsonValue::Bool(false) => Some(0.0),
        JsonValue::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn values_equal(left: &JsonValue, right: &JsonValue) -> bool {
    match (left, right) {
        (JsonValue::Undefined, JsonValue::Undefined) => true,
        (JsonValue::Null, JsonValue::Null) => true,
        (JsonValue::Bool(a), JsonValue::Bool(b)) => a == b,
        (JsonValue::Number(a), JsonValue::Number(b)) => a == b,
        (JsonValue::String(a), JsonValue::String(b)) => a == b,
        _ => false,
    }
}

fn is_truthy(value: &JsonValue) -> bool {
    matches!(core::boolean(value), JsonValue::Bool(true))
}

fn to_sequence(value: &JsonValue) -> Vec<JsonValue> {
    match value {
        JsonValue::Undefined => Vec::new(),
        JsonValue::Array(array) => array.elements.clone(),
        other => vec![other.clone()],
    }
}

fn from_sequence(items: Vec<JsonValue>) -> JsonValue {
    match items.len() {
        0 => JsonValue::Undefined,
        1 => items.into_iter().next().unwrap_or(JsonValue::Undefined),
        _ => JsonValue::Array(JsonArray::new(items, true, false)),
    }
}

fn json_value_from_serde(value: &Value) -> JsonValue {
    match value {
        Value::Null => JsonValue::Null,
        Value::Bool(flag) => JsonValue::Bool(*flag),
        Value::Number(num) => JsonValue::Number(num.as_f64().unwrap_or(0.0)),
        Value::String(text) => JsonValue::String(text.clone()),
        Value::Array(values) => JsonValue::Array(JsonArray::new(
            values.iter().map(json_value_from_serde).collect(),
            false,
            false,
        )),
        Value::Object(map) => JsonValue::Object(JsonObject(
            map.iter()
                .map(|(key, item)| (key.clone(), json_value_from_serde(item)))
                .collect(),
        )),
    }
}
