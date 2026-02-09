use serde_json::{json, Value};

use super::super::super::error::ParserError;
use super::ast_core::process_ast;
use super::common::{ensure_array_field, expr_position, is_type, last_path_step_mut};

pub(super) fn process_bind(expr: Value) -> Result<Value, ParserError> {
    let lhs = expr
        .get("lhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let rhs = expr
        .get("rhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;

    Ok(json!({
        "type": "bind",
        "value": expr.get("value").cloned().unwrap_or(Value::Null),
        "position": expr_position(&expr) as u64,
        "lhs": process_ast(lhs)?,
        "rhs": process_ast(rhs)?,
    }))
}

pub(super) fn process_focus_bind(expr: Value) -> Result<Value, ParserError> {
    let lhs = expr
        .get("lhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let rhs = expr
        .get("rhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;

    let mut result = process_ast(lhs)?;
    let step = if is_type(&result, "path") {
        last_path_step_mut(&mut result, expr_position(&expr))?
    } else {
        &mut result
    };

    let step_map = step
        .as_object_mut()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    if step_map.contains_key("stages") || step_map.contains_key("predicate") {
        return Err(ParserError::new("S0215", expr_position(&expr)));
    }
    if step_map
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|typ| typ == "sort")
    {
        return Err(ParserError::new("S0216", expr_position(&expr)));
    }
    if expr
        .get("keepArray")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        step_map.insert("keepArray".to_string(), Value::Bool(true));
    }
    step_map.insert(
        "focus".to_string(),
        rhs.get("value").cloned().unwrap_or(Value::Null),
    );
    step_map.insert("tuple".to_string(), Value::Bool(true));
    Ok(result)
}

pub(super) fn process_index_bind(expr: Value) -> Result<Value, ParserError> {
    let lhs = expr
        .get("lhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let rhs = expr
        .get("rhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let index_value = rhs.get("value").cloned().unwrap_or(Value::Null);
    let position = expr_position(&expr);

    let mut result = process_ast(lhs)?;
    if !is_type(&result, "path") {
        if let Some(step_map) = result.as_object_mut() {
            if let Some(predicate) = step_map.remove("predicate") {
                step_map.insert("stages".to_string(), predicate);
            }
        }
        let mut wrapped = serde_json::Map::new();
        wrapped.insert("type".to_string(), Value::String("path".to_string()));
        wrapped.insert("steps".to_string(), Value::Array(vec![result]));
        result = Value::Object(wrapped);
    }

    let step = last_path_step_mut(&mut result, position)?;
    let step_map = step
        .as_object_mut()
        .ok_or_else(|| ParserError::new("S0206", position))?;
    if !step_map.contains_key("stages") {
        step_map.insert("index".to_string(), index_value);
    } else {
        let stages = ensure_array_field(step_map, "stages", position)?;
        stages.push(json!({
            "type": "index",
            "value": index_value,
            "position": position as u64,
        }));
    }
    step_map.insert("tuple".to_string(), Value::Bool(true));
    Ok(result)
}

pub(super) fn process_apply(expr: Value) -> Result<Value, ParserError> {
    let lhs = expr
        .get("lhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let rhs = expr
        .get("rhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;

    let lhs = process_ast(lhs)?;
    let rhs = process_ast(rhs)?;
    let keep_array = lhs
        .get("keepArray")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || rhs
            .get("keepArray")
            .and_then(Value::as_bool)
            .unwrap_or(false);

    Ok(json!({
        "type": "apply",
        "value": expr.get("value").cloned().unwrap_or(Value::Null),
        "position": expr_position(&expr) as u64,
        "lhs": lhs,
        "rhs": rhs,
        "keepArray": keep_array,
    }))
}

pub(super) fn process_binary_default(expr: Value) -> Result<Value, ParserError> {
    let lhs = expr
        .get("lhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let rhs = expr
        .get("rhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;

    Ok(json!({
        "type": expr.get("type").cloned().unwrap_or(Value::Null),
        "value": expr.get("value").cloned().unwrap_or(Value::Null),
        "position": expr_position(&expr) as u64,
        "lhs": process_ast(lhs)?,
        "rhs": process_ast(rhs)?,
    }))
}
