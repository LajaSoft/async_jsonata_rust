use serde_json::{json, Value};

use super::ast_core::process_ast;
use super::bindings::{
    process_apply, process_binary_default, process_bind, process_focus_bind, process_index_bind,
};
use super::common::{ensure_array_field, expr_position, is_type, last_path_step_mut, step_position};
use super::super::super::error::ParserError;

pub(super) fn process_binary(expr: Value) -> Result<Value, ParserError> {
    let op = expr
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_default();

    match op {
        "." => process_path(expr),
        "[" => process_predicate(expr),
        "{" => process_group_by(expr),
        "^" => process_order_by(expr),
        ":=" => process_bind(expr),
        "@" => process_focus_bind(expr),
        "#" => process_index_bind(expr),
        "~>" => process_apply(expr),
        _ => process_binary_default(expr),
    }
}

pub(super) fn process_path(expr: Value) -> Result<Value, ParserError> {
    let lhs = expr
        .get("lhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let rhs = expr
        .get("rhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;

    let processed_lhs = process_ast(lhs)?;
    let mut result = if is_type(&processed_lhs, "path") {
        processed_lhs
    } else {
        let mut path = serde_json::Map::new();
        path.insert("type".to_string(), Value::String("path".to_string()));
        path.insert("steps".to_string(), Value::Array(vec![processed_lhs]));
        Value::Object(path)
    };

    let mut processed_rhs = process_ast(rhs)?;
    if is_type(&processed_rhs, "function")
        && processed_rhs
            .get("procedure")
            .and_then(Value::as_object)
            .is_some_and(|procedure| {
                procedure
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|typ| typ == "path")
                    && procedure
                        .get("steps")
                        .and_then(Value::as_array)
                        .is_some_and(|steps| {
                            steps.len() == 1
                                && steps[0]
                                    .get("type")
                                    .and_then(Value::as_str)
                                    .is_some_and(|typ| typ == "name")
                        })
            })
    {
        if let Some(result_steps) = result
            .get_mut("steps")
            .and_then(Value::as_array_mut)
        {
            if let Some(last_step) = result_steps.last_mut() {
                if is_type(last_step, "function") {
                    if let Some(next_function) = processed_rhs
                        .get("procedure")
                        .and_then(Value::as_object)
                        .and_then(|procedure| procedure.get("steps"))
                        .and_then(Value::as_array)
                        .and_then(|steps| steps.first())
                        .and_then(|step| step.get("value"))
                        .cloned()
                    {
                        if let Some(last_step_map) = last_step.as_object_mut() {
                            last_step_map.insert("nextFunction".to_string(), next_function);
                        }
                    }
                }
            }
        }
    }

    if is_type(&processed_rhs, "path") {
        if let Some(rest_steps) = processed_rhs.get("steps").and_then(Value::as_array).cloned() {
            if let Some(result_steps) = result
                .get_mut("steps")
                .and_then(Value::as_array_mut)
            {
                result_steps.extend(rest_steps);
            }
        }
    } else {
        if processed_rhs.get("predicate").is_some() {
            if let Some(rhs_map) = processed_rhs.as_object_mut() {
                if let Some(predicate) = rhs_map.remove("predicate") {
                    rhs_map.insert("stages".to_string(), predicate);
                }
            }
        }
        if let Some(result_steps) = result
            .get_mut("steps")
            .and_then(Value::as_array_mut)
        {
            result_steps.push(processed_rhs);
        }
    }

    let steps = result
        .get_mut("steps")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    for step in steps.iter_mut() {
        let step_type = step
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if step_type == "number" || step_type == "value" {
            return Err(
                ParserError::new("S0213", step_position(step))
                    .with_value(step.get("value").cloned().unwrap_or(Value::Null)),
            );
        }
        if step_type == "string" {
            if let Some(step_map) = step.as_object_mut() {
                step_map.insert("type".to_string(), Value::String("name".to_string()));
            }
        }
    }

    if let Some(first_step) = steps.first_mut() {
        if is_type(first_step, "unary")
            && first_step
                .get("value")
                .and_then(Value::as_str)
                .is_some_and(|value| value == "[")
        {
            if let Some(step_map) = first_step.as_object_mut() {
                step_map.insert("consarray".to_string(), Value::Bool(true));
            }
        }
    }

    if let Some(last_step) = steps.last_mut() {
        if is_type(last_step, "unary")
            && last_step
                .get("value")
                .and_then(Value::as_str)
                .is_some_and(|value| value == "[")
        {
            if let Some(step_map) = last_step.as_object_mut() {
                step_map.insert("consarray".to_string(), Value::Bool(true));
            }
        }
    }

    if steps.iter().any(|step| {
        step.get("keepArray")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }) {
        if let Some(result_map) = result.as_object_mut() {
            result_map.insert("keepSingletonArray".to_string(), Value::Bool(true));
        }
    }

    Ok(result)
}

pub(super) fn process_predicate(expr: Value) -> Result<Value, ParserError> {
    let lhs = expr
        .get("lhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let rhs = expr
        .get("rhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let position = expr_position(&expr);

    let mut result = process_ast(lhs)?;
    let predicate = process_ast(rhs)?;
    let filter = json!({
        "type": "filter",
        "expr": predicate,
        "position": position as u64,
    });

    if is_type(&result, "path") {
        let step = last_path_step_mut(&mut result, position)?;
        let step_map = step
            .as_object_mut()
            .ok_or_else(|| ParserError::new("S0206", position))?;
        if step_map.contains_key("group") {
            return Err(ParserError::new("S0209", position));
        }
        let stages = ensure_array_field(step_map, "stages", position)?;
        stages.push(filter);
        return Ok(result);
    }

    let step_map = result
        .as_object_mut()
        .ok_or_else(|| ParserError::new("S0206", position))?;
    if step_map.contains_key("group") {
        return Err(ParserError::new("S0209", position));
    }
    let predicates = ensure_array_field(step_map, "predicate", position)?;
    predicates.push(filter);
    Ok(result)
}

pub(super) fn process_group_by(expr: Value) -> Result<Value, ParserError> {
    let lhs = expr
        .get("lhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let rhs = expr
        .get("rhs")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;

    let mut result = process_ast(lhs)?;
    let result_map = result
        .as_object_mut()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    if result_map.contains_key("group") {
        return Err(ParserError::new("S0210", expr_position(&expr)));
    }

    let mut group_pairs = Vec::with_capacity(rhs.len());
    for pair in rhs {
        let pair_array = pair
            .as_array()
            .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
        if pair_array.len() != 2 {
            return Err(ParserError::new("S0206", expr_position(&expr)));
        }
        group_pairs.push(Value::Array(vec![
            process_ast(pair_array[0].clone())?,
            process_ast(pair_array[1].clone())?,
        ]));
    }

    result_map.insert(
        "group".to_string(),
        json!({
            "lhs": group_pairs,
            "position": expr_position(&expr) as u64,
        }),
    );

    Ok(result)
}

pub(super) fn process_order_by(expr: Value) -> Result<Value, ParserError> {
    let lhs = expr
        .get("lhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let rhs = expr
        .get("rhs")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;

    let mut result = process_ast(lhs)?;
    if !is_type(&result, "path") {
        let mut wrapped = serde_json::Map::new();
        wrapped.insert("type".to_string(), Value::String("path".to_string()));
        wrapped.insert("steps".to_string(), Value::Array(vec![result]));
        result = Value::Object(wrapped);
    }

    let mut terms = Vec::with_capacity(rhs.len());
    for term in rhs {
        let descending = term
            .get("descending")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let expression = term
            .get("expression")
            .cloned()
            .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
        terms.push(json!({
            "descending": descending,
            "expression": process_ast(expression)?,
        }));
    }

    if let Some(steps) = result.get_mut("steps").and_then(Value::as_array_mut) {
        steps.push(json!({
            "type": "sort",
            "position": expr_position(&expr) as u64,
            "terms": terms,
        }));
    }

    Ok(result)
}

