use std::cmp::Ordering;
use std::collections::HashMap;

use serde_json::Value;

use crate::error::Error;
use crate::functions::core;
use crate::types::{JsonArray, JsonFunction, JsonObject, JsonValue};

use super::ops::{compare_sort_values, from_sequence, is_truthy, to_sequence};
use super::value::{object_keys_from_value, upsert_object_property};
use super::{eval, Bindings};

pub(super) fn eval_path(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let steps = node
        .get("steps")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2002", "Path node missing steps"))?;
    let starts_from_variable = steps
        .first()
        .and_then(|step| step.get("type").and_then(Value::as_str))
        == Some("variable");
    let mut current = match focus {
        JsonValue::Array(array) if !array.is_sequence && !starts_from_variable => {
            JsonValue::Array(JsonArray::new(
                array.elements.clone(),
                true,
                array.outer_wrapper,
            ))
        }
        _ => focus.clone(),
    };

    for (index, step) in steps.iter().enumerate() {
        if index == 0
            && step
                .get("consarray")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        {
            current = eval(step, input, &current, functions, bindings)?;
            continue;
        }
        current = eval_path_step(step, input, &current, functions, bindings)?;
    }

    Ok(current)
}

fn eval_path_step(
    step: &Value,
    input: &JsonValue,
    current: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
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
        "function" => eval_path_expr_step(step, input, current, functions, bindings)?,
        "variable" => eval(step, input, current, functions, bindings)?,
        "block" => {
            let has_local_predicate = step
                .get("predicate")
                .and_then(Value::as_array)
                .map(|predicates| !predicates.is_empty())
                .unwrap_or(false);
            if has_local_predicate {
                eval(step, input, current, functions, bindings)?
            } else {
                eval_path_expr_step(step, input, current, functions, bindings)?
            }
        }
        "condition" => eval_path_expr_step(step, input, current, functions, bindings)?,
        "number" => JsonValue::Number(step.get("value").and_then(Value::as_f64).unwrap_or(0.0)),
        "string" => JsonValue::String(
            step.get("value")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        ),
        "sort" => apply_sort_step(step, input, current, functions, bindings)?,
        "wildcard" => apply_wildcard(current),
        "unary" => eval_path_expr_step(step, input, current, functions, bindings)?,
        "descendant" => apply_descendant(current),
        other => {
            if step.get("type").is_some() {
                eval_path_expr_step(step, input, current, functions, bindings)?
            } else {
                return Err(Error::new(
                    "E2003",
                    format!("Unsupported path step type: {other}"),
                ));
            }
        }
    };

    if let Some(index) = step.get("index") {
        out = apply_index(&out, index);
    }

    if let Some(stages) = step.get("stages").and_then(Value::as_array) {
        for stage in stages {
            out = apply_stage(stage, input, &out, functions, bindings)?;
        }
    }

    Ok(out)
}

fn apply_stage(
    stage: &Value,
    input: &JsonValue,
    current: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
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

            if let Some(index) = extract_filter_index(expr) {
                if let JsonValue::Array(array) = current {
                    if array.is_sequence
                        && array
                            .elements
                            .iter()
                            .all(|item| matches!(item, JsonValue::Array(_)))
                    {
                        let mut mapped = Vec::new();
                        let index_value = Value::Number(index.into());
                        for item in &array.elements {
                            let selected = apply_index(item, &index_value);
                            if !selected.is_undefined() {
                                mapped.push(selected);
                            }
                        }
                        return Ok(from_sequence(mapped));
                    }
                }
                return Ok(apply_index(current, &Value::Number(index.into())));
            }

            let mut kept = Vec::new();
            for item in to_sequence(current) {
                let predicate = eval(expr, input, &item, functions, bindings)?;
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

pub(super) fn apply_predicate_expr(
    expr: &Value,
    input: &JsonValue,
    current: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    if let Some(index) = extract_filter_index(expr) {
        return Ok(apply_index(current, &Value::Number(index.into())));
    }

    let mut kept = Vec::new();
    for item in to_sequence(current) {
        let predicate = eval(expr, input, &item, functions, bindings)?;
        if is_truthy(&predicate) {
            kept.push(item);
        }
    }
    Ok(from_sequence(kept))
}

fn eval_path_expr_step(
    step: &Value,
    input: &JsonValue,
    current: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let mut values = to_sequence(current);
    if values.is_empty() && current.is_undefined() {
        values.push(JsonValue::Undefined);
    }
    if values.is_empty() {
        return Ok(JsonValue::Undefined);
    }

    let mut out = Vec::new();
    for item in values {
        let value = eval(step, input, &item, functions, bindings)?;
        if value.is_undefined() {
            continue;
        }
        match value {
            JsonValue::Array(array) if array.is_sequence => {
                for element in array.elements {
                    out.push(element);
                }
            }
            other => out.push(other),
        }
    }

    Ok(from_sequence(out))
}

pub(super) fn apply_group_expression(
    group: &Value,
    input: &JsonValue,
    current: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let pairs = group
        .get("lhs")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2030", "Group expression missing lhs"))?;
    if pairs.is_empty() {
        return Ok(JsonValue::Undefined);
    }

    let items = to_sequence(current);
    if items.is_empty() {
        return Ok(JsonValue::Undefined);
    }

    let mut result = JsonObject(Vec::new());
    for pair in pairs {
        let pair_values = pair
            .as_array()
            .ok_or_else(|| Error::new("E2031", "Group pair must be array"))?;
        if pair_values.len() != 2 {
            return Err(Error::new("E2032", "Group pair must contain key and value"));
        }

        let key_expr = &pair_values[0];
        let value_expr = &pair_values[1];

        let mut grouped: HashMap<String, Vec<JsonValue>> = HashMap::new();
        let mut key_order: Vec<String> = Vec::new();

        for item in &items {
            let key_value = eval(key_expr, input, item, functions, bindings)?;
            let keys = object_keys_from_value(&key_value);
            for key in keys {
                if !grouped.contains_key(&key) {
                    key_order.push(key.clone());
                }
                grouped.entry(key).or_default().push(item.clone());
            }
        }

        for key in key_order {
            let grouped_items = grouped.remove(&key).unwrap_or_default();
            let grouped_context = JsonValue::Array(JsonArray::new(grouped_items, true, false));
            let value = eval(value_expr, input, &grouped_context, functions, bindings)?;
            upsert_object_property(&mut result, key, value);
        }
    }

    Ok(JsonValue::Object(result))
}

fn extract_filter_index(expr: &Value) -> Option<i64> {
    if expr.get("type").and_then(Value::as_str) == Some("number") {
        return numeric_index(expr.get("value"));
    }

    if expr.get("type").and_then(Value::as_str) == Some("value") {
        return numeric_index(expr.get("value"));
    }

    None
}

fn numeric_index(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    if let Some(index) = value.as_i64() {
        return Some(index);
    }
    let float = value.as_f64()?;
    if float.fract() != 0.0 {
        return None;
    }
    Some(float as i64)
}

fn apply_index(current: &JsonValue, index: &Value) -> JsonValue {
    let Some(idx) = numeric_index(Some(index)) else {
        return JsonValue::Undefined;
    };

    let items: Vec<JsonValue> = match current {
        JsonValue::Array(array) if !array.is_sequence => array.elements.clone(),
        _ => to_sequence(current),
    };
    if items.is_empty() {
        return JsonValue::Undefined;
    }

    let position = if idx < 0 {
        let from_end = items.len() as i64 + idx;
        if from_end < 0 {
            return JsonValue::Undefined;
        }
        from_end as usize
    } else {
        idx as usize
    };

    items
        .get(position)
        .cloned()
        .unwrap_or(JsonValue::Undefined)
}

fn apply_sort_step(
    step: &Value,
    input: &JsonValue,
    current: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let terms = step
        .get("terms")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2029", "Sort step missing terms"))?;

    let mut values = to_sequence(current);
    values.sort_by(|left, right| {
        for term in terms {
            let descending = term
                .get("descending")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let expr = match term.get("expression") {
                Some(expr) => expr,
                None => continue,
            };

            let left_value = eval(expr, input, left, functions, bindings).ok();
            let right_value = eval(expr, input, right, functions, bindings).ok();
            let mut ordering = compare_sort_values(left_value.as_ref(), right_value.as_ref());
            if descending {
                ordering = ordering.reverse();
            }
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
        Ordering::Equal
    });

    Ok(from_sequence(values))
}

pub(super) fn apply_wildcard(current: &JsonValue) -> JsonValue {
    match current {
        JsonValue::Object(JsonObject(entries)) => {
            let values = entries.iter().map(|(_, value)| value.clone()).collect();
            from_sequence(values)
        }
        JsonValue::Array(array) => {
            let mut values = Vec::new();
            for item in &array.elements {
                match item {
                    JsonValue::Object(JsonObject(entries)) => {
                        for (_, value) in entries {
                            values.push(value.clone());
                        }
                    }
                    other => values.push(other.clone()),
                }
            }
            from_sequence(values)
        }
        _ => JsonValue::Undefined,
    }
}

pub(super) fn apply_descendant(current: &JsonValue) -> JsonValue {
    let mut out = Vec::new();
    if matches!(current, JsonValue::Object(_) | JsonValue::Array(_)) {
        out.push(current.clone());
    }
    collect_descendants(current, &mut out);
    from_sequence(out)
}

fn collect_descendants(value: &JsonValue, out: &mut Vec<JsonValue>) {
    match value {
        JsonValue::Array(array) => {
            for item in &array.elements {
                collect_descendants(item, out);
            }
        }
        JsonValue::Object(JsonObject(entries)) => {
            for (_, entry) in entries {
                out.push(entry.clone());
                collect_descendants(entry, out);
            }
        }
        _ => {}
    }
}
