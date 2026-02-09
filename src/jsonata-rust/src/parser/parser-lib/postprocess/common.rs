use serde_json::Value;

use super::super::super::error::ParserError;

pub(super) fn map_position(map: &serde_json::Map<String, Value>) -> usize {
    map.get("position").and_then(Value::as_u64).unwrap_or(0) as usize
}

pub(super) fn is_type(value: &Value, expected: &str) -> bool {
    value
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|typ| typ == expected)
}

pub(super) fn ensure_array_field<'a>(
    map: &'a mut serde_json::Map<String, Value>,
    name: &str,
    position: usize,
) -> Result<&'a mut Vec<Value>, ParserError> {
    let entry = map
        .entry(name.to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    entry
        .as_array_mut()
        .ok_or_else(|| ParserError::new("S0206", position))
}

pub(super) fn last_path_step_mut(
    path: &mut Value,
    position: usize,
) -> Result<&mut Value, ParserError> {
    let path_map = path
        .as_object_mut()
        .ok_or_else(|| ParserError::new("S0206", position))?;
    let steps = path_map
        .get_mut("steps")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| ParserError::new("S0206", position))?;
    steps
        .last_mut()
        .ok_or_else(|| ParserError::new("S0206", position))
}

pub(crate) fn expr_position(expr: &Value) -> usize {
    expr.get("position").and_then(Value::as_u64).unwrap_or(0) as usize
}

pub(super) fn step_position(step: &Value) -> usize {
    step.get("position").and_then(Value::as_u64).unwrap_or(0) as usize
}
