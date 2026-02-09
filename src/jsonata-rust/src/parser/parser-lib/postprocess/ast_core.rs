use serde_json::Value;

use super::super::super::error::ParserError;
use super::common::{is_type, map_position};
use super::path_ops::process_binary;

pub(crate) fn process_ast(expr: Value) -> Result<Value, ParserError> {
    let keep_array = expr
        .get("keepArray")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut result = match expr {
        Value::Object(map) => process_ast_object(map)?,
        other => other,
    };

    if keep_array {
        if let Some(result_map) = result.as_object_mut() {
            result_map.insert("keepArray".to_string(), Value::Bool(true));
        }
    }

    Ok(result)
}

fn process_ast_object(mut map: serde_json::Map<String, Value>) -> Result<Value, ParserError> {
    let expr_type = map.get("type").and_then(Value::as_str).unwrap_or_default();

    match expr_type {
        "binary" => process_binary(Value::Object(map)),
        "unary" => process_unary(map),
        "function" | "partial" => process_function_or_partial(map),
        "lambda" => process_lambda(map),
        "condition" => process_condition(map),
        "transform" => process_transform(map),
        "block" => process_block(map),
        "path" => process_path_value(map),
        "name" => {
            let mut result = serde_json::Map::new();
            let step = Value::Object(map);
            let keep_singleton = step
                .get("keepArray")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            result.insert("type".to_string(), Value::String("path".to_string()));
            result.insert("steps".to_string(), Value::Array(vec![step]));
            if keep_singleton {
                result.insert("keepSingletonArray".to_string(), Value::Bool(true));
            }
            Ok(Value::Object(result))
        }
        "string" | "number" | "value" | "wildcard" | "descendant" | "variable" | "regex"
        | "parent" => Ok(Value::Object(map)),
        "operator" => process_operator(map),
        "error" => {
            if let Some(lhs) = map.remove("lhs") {
                return process_ast(lhs);
            }
            Ok(Value::Object(map))
        }
        _ => {
            let mut code = "S0206";
            if map
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| id == "(end)")
            {
                code = "S0207";
            }
            Err(ParserError::new(code, map_position(&map))
                .with_token(map.get("value").cloned().unwrap_or(Value::Null)))
        }
    }
}

fn process_unary(mut map: serde_json::Map<String, Value>) -> Result<Value, ParserError> {
    let mut result = serde_json::Map::new();
    let value = map.get("value").cloned().unwrap_or(Value::Null);
    let position = map_position(&map);
    result.insert("type".to_string(), Value::String("unary".to_string()));
    result.insert("value".to_string(), value.clone());
    result.insert(
        "position".to_string(),
        Value::Number(serde_json::Number::from(position as u64)),
    );

    let unary_value = value.as_str().unwrap_or_default();
    if unary_value == "[" {
        let mut expressions = Vec::new();
        for item in map
            .remove("expressions")
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
        {
            expressions.push(process_ast(item)?);
        }
        result.insert("expressions".to_string(), Value::Array(expressions));
        return Ok(Value::Object(result));
    }

    if unary_value == "{" {
        let mut pairs = Vec::new();
        for pair in map
            .remove("lhs")
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
        {
            let pair_array = pair
                .as_array()
                .ok_or_else(|| ParserError::new("S0206", position))?;
            if pair_array.len() != 2 {
                return Err(ParserError::new("S0206", position));
            }
            pairs.push(Value::Array(vec![
                process_ast(pair_array[0].clone())?,
                process_ast(pair_array[1].clone())?,
            ]));
        }
        result.insert("lhs".to_string(), Value::Array(pairs));
        return Ok(Value::Object(result));
    }

    let expression = map
        .remove("expression")
        .ok_or_else(|| ParserError::new("S0206", position))?;
    let expression = process_ast(expression)?;
    if unary_value == "-" && is_type(&expression, "number") {
        let mut number = expression;
        if let Some(number_map) = number.as_object_mut() {
            if let Some(value) = number_map.get("value").and_then(Value::as_f64) {
                if let Some(number_value) = serde_json::Number::from_f64(-value) {
                    number_map.insert("value".to_string(), Value::Number(number_value));
                }
            }
        }
        return Ok(number);
    }
    result.insert("expression".to_string(), expression);
    Ok(Value::Object(result))
}

fn process_function_or_partial(
    mut map: serde_json::Map<String, Value>,
) -> Result<Value, ParserError> {
    let expr_type = map
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut result = serde_json::Map::new();
    result.insert("type".to_string(), Value::String(expr_type));
    if let Some(name) = map.get("name").cloned() {
        result.insert("name".to_string(), name);
    }
    if let Some(value) = map.get("value").cloned() {
        result.insert("value".to_string(), value);
    }
    result.insert(
        "position".to_string(),
        Value::Number(serde_json::Number::from(map_position(&map) as u64)),
    );

    let mut arguments = Vec::new();
    for argument in map
        .remove("arguments")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
    {
        arguments.push(process_ast(argument)?);
    }
    result.insert("arguments".to_string(), Value::Array(arguments));
    let procedure = map
        .remove("procedure")
        .ok_or_else(|| ParserError::new("S0206", map_position(&map)))?;
    result.insert("procedure".to_string(), process_ast(procedure)?);
    Ok(Value::Object(result))
}

fn process_lambda(mut map: serde_json::Map<String, Value>) -> Result<Value, ParserError> {
    let mut result = serde_json::Map::new();
    result.insert("type".to_string(), Value::String("lambda".to_string()));
    result.insert(
        "position".to_string(),
        Value::Number(serde_json::Number::from(map_position(&map) as u64)),
    );
    if let Some(arguments) = map.remove("arguments") {
        result.insert("arguments".to_string(), arguments);
    }
    if let Some(signature) = map.remove("signature") {
        result.insert("signature".to_string(), signature);
    }
    let body = map
        .remove("body")
        .ok_or_else(|| ParserError::new("S0206", map_position(&map)))?;
    let body = process_ast(body)?;
    result.insert("body".to_string(), tail_call_optimize(body));
    Ok(Value::Object(result))
}

fn tail_call_optimize(expr: Value) -> Value {
    let Value::Object(mut map) = expr else {
        return expr;
    };

    let expr_type = map
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    if expr_type == "function" && !map.contains_key("predicate") {
        let position = map.get("position").cloned().unwrap_or(Value::Null);
        let mut thunk = serde_json::Map::new();
        thunk.insert("type".to_string(), Value::String("lambda".to_string()));
        thunk.insert("thunk".to_string(), Value::Bool(true));
        thunk.insert("arguments".to_string(), Value::Array(vec![]));
        thunk.insert("position".to_string(), position);
        thunk.insert("body".to_string(), Value::Object(map));
        return Value::Object(thunk);
    }

    if expr_type == "condition" {
        if let Some(then_branch) = map.remove("then") {
            map.insert("then".to_string(), tail_call_optimize(then_branch));
        }
        if let Some(else_branch) = map.remove("else") {
            map.insert("else".to_string(), tail_call_optimize(else_branch));
        }
        return Value::Object(map);
    }

    if expr_type == "block" {
        if let Some(expressions) = map.get_mut("expressions").and_then(Value::as_array_mut) {
            if let Some(last) = expressions.pop() {
                expressions.push(tail_call_optimize(last));
            }
        }
        return Value::Object(map);
    }

    Value::Object(map)
}

fn process_condition(mut map: serde_json::Map<String, Value>) -> Result<Value, ParserError> {
    let mut result = serde_json::Map::new();
    result.insert("type".to_string(), Value::String("condition".to_string()));
    result.insert(
        "position".to_string(),
        Value::Number(serde_json::Number::from(map_position(&map) as u64)),
    );
    let condition = map
        .remove("condition")
        .ok_or_else(|| ParserError::new("S0206", map_position(&map)))?;
    let then_branch = map
        .remove("then")
        .ok_or_else(|| ParserError::new("S0206", map_position(&map)))?;
    result.insert("condition".to_string(), process_ast(condition)?);
    result.insert("then".to_string(), process_ast(then_branch)?);
    if let Some(else_branch) = map.remove("else") {
        result.insert("else".to_string(), process_ast(else_branch)?);
    }
    Ok(Value::Object(result))
}

fn process_transform(mut map: serde_json::Map<String, Value>) -> Result<Value, ParserError> {
    let mut result = serde_json::Map::new();
    result.insert("type".to_string(), Value::String("transform".to_string()));
    result.insert(
        "position".to_string(),
        Value::Number(serde_json::Number::from(map_position(&map) as u64)),
    );
    let pattern = map
        .remove("pattern")
        .ok_or_else(|| ParserError::new("S0206", map_position(&map)))?;
    let update = map
        .remove("update")
        .ok_or_else(|| ParserError::new("S0206", map_position(&map)))?;
    result.insert("pattern".to_string(), process_ast(pattern)?);
    result.insert("update".to_string(), process_ast(update)?);
    if let Some(delete_expr) = map.remove("delete") {
        result.insert("delete".to_string(), process_ast(delete_expr)?);
    }
    Ok(Value::Object(result))
}

fn process_block(mut map: serde_json::Map<String, Value>) -> Result<Value, ParserError> {
    let mut result = serde_json::Map::new();
    result.insert("type".to_string(), Value::String("block".to_string()));
    result.insert(
        "position".to_string(),
        Value::Number(serde_json::Number::from(map_position(&map) as u64)),
    );

    let mut expressions = Vec::new();
    let mut has_consarray = false;
    for item in map
        .remove("expressions")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
    {
        let part = process_ast(item)?;
        if part
            .get("consarray")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            has_consarray = true;
        }
        if is_type(&part, "path") {
            if let Some(first_step) = part
                .get("steps")
                .and_then(Value::as_array)
                .and_then(|steps| steps.first())
            {
                if first_step
                    .get("consarray")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    has_consarray = true;
                }
            }
        }
        expressions.push(part);
    }

    result.insert("expressions".to_string(), Value::Array(expressions));
    if has_consarray {
        result.insert("consarray".to_string(), Value::Bool(true));
    }
    Ok(Value::Object(result))
}

fn process_path_value(mut map: serde_json::Map<String, Value>) -> Result<Value, ParserError> {
    let mut steps = Vec::new();
    for step in map
        .remove("steps")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
    {
        steps.push(process_ast(step)?);
    }
    map.insert("steps".to_string(), Value::Array(steps));
    Ok(Value::Object(map))
}

fn process_operator(mut map: serde_json::Map<String, Value>) -> Result<Value, ParserError> {
    let value = map.get("value").and_then(Value::as_str).unwrap_or_default();
    if value == "and" || value == "or" || value == "in" {
        map.insert("type".to_string(), Value::String("name".to_string()));
        return process_ast(Value::Object(map));
    }
    if value == "?" {
        return Ok(Value::Object(map));
    }
    Err(ParserError::new("S0201", map_position(&map))
        .with_token(map.get("value").cloned().unwrap_or(Value::Null)))
}
