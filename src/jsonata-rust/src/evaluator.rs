use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::error::Error;
use crate::functions::core;
use crate::types::{JsonFunction, JsonValue};

mod callable;
mod expressions;
mod lambda;
mod ops;
mod path;
mod transform;
mod value;

type Bindings = HashMap<String, JsonValue>;
const EVAL_MILLIS_BINDING: &str = "__jsonata_eval_millis";

fn monotonic_eval_millis() -> i64 {
    static LAST_MILLIS: AtomicI64 = AtomicI64::new(0);
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0);
    loop {
        let previous = LAST_MILLIS.load(Ordering::Relaxed);
        let candidate = if now_ms > previous {
            now_ms
        } else {
            previous + 1
        };
        if LAST_MILLIS
            .compare_exchange(previous, candidate, Ordering::SeqCst, Ordering::Relaxed)
            .is_ok()
        {
            return candidate;
        }
    }
}

pub(crate) fn evaluate_expression(
    ast: &Value,
    input: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
) -> Result<JsonValue, Error> {
    let bindings = Bindings::new();
    evaluate_expression_with_bindings(ast, input, functions, &bindings)
}

pub(crate) fn evaluate_expression_with_bindings(
    ast: &Value,
    input: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &HashMap<String, JsonValue>,
) -> Result<JsonValue, Error> {
    let mut eval_bindings = bindings.clone();
    if !eval_bindings.contains_key(EVAL_MILLIS_BINDING) {
        eval_bindings.insert(
            EVAL_MILLIS_BINDING.to_owned(),
            JsonValue::Number(monotonic_eval_millis() as f64),
        );
    }
    eval(ast, input, input, functions, &eval_bindings)
}

pub(super) fn eval(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let node_type = node.get("type").and_then(Value::as_str).unwrap_or_default();

    let mut result = match node_type {
        "path" => path::eval_path(node, input, focus, functions, bindings),
        "name" => {
            let name = node
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Ok(core::lookup(focus, name))
        }
        "variable" => callable::eval_variable(node, input, focus, functions, bindings),
        "string" => Ok(JsonValue::String(
            node.get("value")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        )),
        "number" => Ok(JsonValue::Number(
            node.get("value").and_then(Value::as_f64).unwrap_or(0.0),
        )),
        "value" => Ok(value::json_value_from_serde(
            node.get("value").unwrap_or(&Value::Null),
        )),
        "regex" => callable::eval_regex(node),
        "function" => callable::eval_function(node, input, focus, functions, bindings),
        "binary" => ops::eval_binary(node, input, focus, functions, bindings),
        "apply" => callable::eval_apply(node, input, focus, functions, bindings),
        "block" => expressions::eval_block(node, input, focus, functions, bindings),
        "unary" => expressions::eval_unary(node, input, focus, functions, bindings),
        "bind" => {
            let (_, value) = expressions::eval_bind(node, input, focus, functions, bindings)?;
            Ok(value)
        }
        "lambda" => lambda::eval_lambda(node, input, focus, functions, bindings),
        "condition" => expressions::eval_condition(node, input, focus, functions, bindings),
        "wildcard" => Ok(path::apply_wildcard(focus)),
        "descendant" => Ok(path::apply_descendant(focus)),
        _ => Err(Error::new(
            "E2001",
            format!("Unsupported AST node type: {node_type}"),
        )),
    }?;

    if let Some(predicates) = node.get("predicate").and_then(Value::as_array) {
        for predicate in predicates {
            let expr = predicate.get("expr").unwrap_or(predicate);
            result = path::apply_predicate_expr(expr, input, &result, functions, bindings)?;
        }
    }

    if node_type == "path" {
        if let Some(group) = node.get("group") {
            result = path::apply_group_expression(group, input, &result, functions, bindings)?;
        }
    }

    Ok(ops::normalize_sequence(result))
}
