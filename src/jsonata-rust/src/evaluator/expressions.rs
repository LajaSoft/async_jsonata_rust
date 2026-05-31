use std::collections::HashMap;

use futures::future::BoxFuture;
use serde_json::Value;

use crate::error::Error;
use crate::types::{JsonArray, JsonFunction, JsonObject, JsonValue};

use super::lambda;
use super::value::{materialize_value, upsert_object_property};
use super::{eval, Bindings};

pub(super) fn eval_block<'a>(
    node: &'a Value,
    input: &'a JsonValue,
    focus: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    Box::pin(async move {
    let expressions = node
        .get("expressions")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2015", "Block node missing expressions"))?;

    let mut local_bindings = bindings.clone();
    let mut last = JsonValue::Undefined;
    // A single shared, mutable environment frame for this block. Every function
    // bound via `:=` in this block captures a clone of this Arc; whenever a new
    // function is bound we also record it into the frame, so earlier-defined
    // sibling functions can resolve later-defined ones at call time (let-rec /
    // mutual recursion). Created lazily on first function binding.
    let mut shared_frame: Option<std::sync::Arc<std::sync::RwLock<Bindings>>> = None;
    for expr in expressions {
        if expr.get("type").and_then(Value::as_str) == Some("bind") {
            // Collect every variable name along a chain of nested binds
            // (`$a := $b := expr`) so each receives the evaluated value and is
            // visible in the enclosing block, mirroring upstream's mutable
            // environment frame where every `:=` binds into the same scope.
            let mut names: Vec<String> = Vec::new();
            let mut current = expr;
            loop {
                let lhs = current
                    .get("lhs")
                    .ok_or_else(|| Error::new("E2022", "Bind node missing lhs"))?;
                let name = lhs
                    .get("value")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Error::new("E2024", "Bind lhs must be variable"))?
                    .trim_start_matches('$')
                    .to_owned();
                if name.is_empty() {
                    return Err(Error::new("E2025", "Bind variable name is empty"));
                }
                names.push(name);
                let rhs = current
                    .get("rhs")
                    .ok_or_else(|| Error::new("E2023", "Bind node missing rhs"))?;
                if rhs.get("type").and_then(Value::as_str) == Some("bind") {
                    current = rhs;
                    continue;
                }
                break;
            }
            let rhs = current.get("rhs").unwrap();
            let mut value = eval(rhs, input, focus, functions, &local_bindings).await?;
            for name in &names {
                let mut bound = value.clone();
                if let JsonValue::Function(function) = &bound {
                    // Tie the recursion knot first (self-reference), then attach
                    // the block's shared let-rec frame so this lambda can also
                    // see sibling functions defined later in the same block.
                    let mut func = function.clone();
                    if let Some(rebound) = lambda::bind_recursive_name(&func, name) {
                        func = rebound;
                    }
                    if lambda::is_lambda_function(&func) {
                        let frame = shared_frame
                            .get_or_insert_with(lambda::new_shared_frame)
                            .clone();
                        if let Some(rebound) = lambda::attach_shared_frame(&func, &frame) {
                            func = rebound;
                        }
                    }
                    bound = JsonValue::Function(func);
                }
                local_bindings.insert(name.clone(), bound.clone());
                local_bindings.insert(format!("${name}"), bound.clone());
                // Record into the shared frame so previously-defined sibling
                // functions (which captured the same Arc) can resolve this name.
                if let (JsonValue::Function(_), Some(frame)) = (&bound, &shared_frame) {
                    if let Ok(mut frame) = frame.write() {
                        frame.insert(name.clone(), bound.clone());
                        frame.insert(format!("${name}"), bound.clone());
                    }
                }
                value = bound;
            }
            last = value;
            continue;
        }
        last = eval(expr, input, focus, functions, &local_bindings).await?;
    }

    Ok(last)
    })
}

pub(super) fn eval_unary<'a>(
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

    if op == "[" {
        let expressions = node
            .get("expressions")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::new("E2016", "Array unary missing expressions"))?;
        let mut out = Vec::with_capacity(expressions.len());
        for expr in expressions {
            let value = eval(expr, input, focus, functions, bindings).await?;
            if value.is_undefined() {
                continue;
            }
            // JSONata array-constructor semantics (evaluateUnary, case '['):
            // if the sub-expression is itself an array literal, push its value
            // as a single nested element; otherwise use `append` which flattens
            // any array (sequence or materialized) into the result.
            let is_array_literal = expr.get("type").and_then(Value::as_str) == Some("unary")
                && expr.get("value").and_then(Value::as_str) == Some("[");
            if is_array_literal {
                out.push(value);
                continue;
            }
            match value {
                JsonValue::Array(array) => {
                    for element in array.elements {
                        out.push(element);
                    }
                }
                other => out.push(other),
            }
        }
        // Upstream only marks the array `cons` (preventing path-step flattening)
        // when the array constructor sits at the head or tail of a path; the
        // parser records this with `consarray`. Plain array literals are NOT
        // cons arrays, so they flatten like any other array in path steps.
        let cons = node
            .get("consarray")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        return Ok(JsonValue::Array(JsonArray::new(out, false, cons)));
    }

    if op == "{" {
        let pairs = node
            .get("lhs")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::new("E2019", "Object unary missing lhs"))?;
        let mut object = JsonObject(Vec::new());
        // Track which pair (expression index) produced each key so duplicate
        // keys from different expressions raise D1009 (matches the oracle).
        let mut key_owner: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (pair_index, pair) in pairs.iter().enumerate() {
            let pair_values = pair
                .as_array()
                .ok_or_else(|| Error::new("E2020", "Object pair must be array"))?;
            if pair_values.len() != 2 {
                return Err(Error::new(
                    "E2021",
                    "Object pair must contain key and value",
                ));
            }

            let key_value = eval(&pair_values[0], input, focus, functions, bindings).await?;
            // The key must evaluate to a string (or be absent); anything else is
            // a T1003 error.
            let key = match key_value {
                JsonValue::String(text) => Some(text),
                JsonValue::Undefined => {
                    extract_object_literal_key(&pair_values[0])
                }
                _ => {
                    return Err(Error::new(
                        "T1003",
                        "Key in object structure must evaluate to a string",
                    ))
                }
            };
            let Some(key) = key else {
                continue;
            };

            let value = eval(&pair_values[1], input, focus, functions, bindings).await?;
            let value = materialize_value(&value);
            match key_owner.get(&key) {
                Some(&owner) if owner != pair_index => {
                    return Err(Error::new(
                        "D1009",
                        "Multiple key definitions evaluate to same key in object constructor",
                    ));
                }
                _ => {}
            }
            key_owner.insert(key.clone(), pair_index);
            upsert_object_property(&mut object, key, value);
        }
        return Ok(JsonValue::Object(object));
    }

    if op == "-" {
        let expr = node
            .get("expression")
            .ok_or_else(|| Error::new("E2017", "Unary minus missing expression"))?;
        let value = eval(expr, input, focus, functions, bindings).await?;
        if value.is_undefined() {
            return Ok(JsonValue::Undefined);
        }
        if let JsonValue::Number(num) = value {
            if num.is_finite() {
                return Ok(JsonValue::Number(-num));
            }
            return Err(Error::new("D1001", format!("Number out of range;value:{num}")));
        }
        return Err(Error::new("D1002", "Cannot negate a non-numeric value;token:-"));
    }

    Err(Error::new(
        "E2018",
        format!("Unsupported unary operator: {op}"),
    ))
    })
}

pub(super) fn eval_bind<'a>(
    node: &'a Value,
    input: &'a JsonValue,
    focus: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<(String, JsonValue), Error>> {
    Box::pin(async move {
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

    let value = eval(rhs, input, focus, functions, bindings).await?;
    Ok((name, value))
    })
}

pub(super) fn eval_condition<'a>(
    node: &'a Value,
    input: &'a JsonValue,
    focus: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    Box::pin(async move {
    let condition = node
        .get("condition")
        .ok_or_else(|| Error::new("E2027", "Condition node missing condition"))?;
    let then_branch = node
        .get("then")
        .ok_or_else(|| Error::new("E2028", "Condition node missing then"))?;

    let predicate = eval(condition, input, focus, functions, bindings).await?;
    if super::ops::is_truthy(&predicate) {
        return eval(then_branch, input, focus, functions, bindings).await;
    }

    if let Some(else_branch) = node.get("else") {
        return eval(else_branch, input, focus, functions, bindings).await;
    }

    Ok(JsonValue::Undefined)
    })
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
