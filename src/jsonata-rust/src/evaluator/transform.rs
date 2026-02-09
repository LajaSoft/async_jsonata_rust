use std::collections::HashMap;

use futures::executor::block_on;
use serde_json::Value;

use crate::error::Error;
use crate::types::{FunctionContext, JsonArray, JsonFunction, JsonObject, JsonValue, JsonataFocus};

use super::ops::{to_sequence, values_equal};
use super::value::object_keys_from_value;
use super::{eval, Bindings};

pub(super) fn eval_transform_apply(
    transform: &Value,
    input: &JsonValue,
    base: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let mut target_base = base.clone();
    if let Some(clone_binding) = bindings
        .get("$clone")
        .or_else(|| bindings.get("clone"))
        .cloned()
    {
        let JsonValue::Function(callable) = clone_binding else {
            return Err(Error::new("T2013", "The transform expression cloned the input using $clone(), but this was overridden by a non-function value"));
        };
        let ctx = FunctionContext::with_focus(JsonataFocus::new(base.clone()));
        target_base = block_on(callable.call(ctx, vec![base.clone()])).map_err(Error::from)?;
    } else if let Some(callable) = functions.get("clone") {
        let ctx = FunctionContext::with_focus(JsonataFocus::new(base.clone()));
        target_base = block_on(callable.call(ctx, vec![base.clone()])).map_err(Error::from)?;
    }

    let pattern = transform
        .get("pattern")
        .ok_or_else(|| Error::new("T2010", "Transform expression missing pattern"))?;
    let update = transform
        .get("update")
        .ok_or_else(|| Error::new("T2011", "Transform expression missing update clause"))?;
    let delete = transform.get("delete");

    let matches_value = eval(pattern, input, &target_base, functions, bindings)?;
    let matches = to_sequence(&matches_value);
    if matches.is_empty() {
        return Ok(target_base);
    }

    let mut ops = Vec::new();
    for target in matches {
        let update_value = eval(update, input, &target, functions, bindings)?;
        let update_object = match update_value {
            JsonValue::Undefined => JsonObject(Vec::new()),
            JsonValue::Object(object) => object,
            _ => {
                return Err(Error::new(
                    "T2011",
                    "Transform update clause must evaluate to an object",
                ));
            }
        };

        let mut delete_keys = Vec::new();
        if let Some(delete_expr) = delete {
            let delete_value = eval(delete_expr, input, &target, functions, bindings)?;
            delete_keys = object_keys_from_value(&delete_value);
            let valid_delete = matches!(
                delete_value,
                JsonValue::String(_) | JsonValue::Array(_) | JsonValue::Undefined
            );
            if !valid_delete {
                return Err(Error::new(
                    "T2012",
                    "Transform delete clause must evaluate to a string or array of strings",
                ));
            }
        }

        ops.push(TransformOp {
            target,
            update: update_object,
            delete_keys,
        });
    }

    Ok(apply_transform_ops(&target_base, &ops))
}

#[derive(Clone)]
struct TransformOp {
    target: JsonValue,
    update: JsonObject,
    delete_keys: Vec<String>,
}

fn apply_transform_ops(base: &JsonValue, ops: &[TransformOp]) -> JsonValue {
    let mut transformed = base.clone();
    for op in ops {
        transformed = apply_single_transform_op(&transformed, op);
    }
    transformed
}

fn apply_single_transform_op(value: &JsonValue, op: &TransformOp) -> JsonValue {
    if values_equal(value, &op.target) {
        return apply_transform_to_value(value, &op.update, &op.delete_keys);
    }

    match value {
        JsonValue::Array(array) => {
            let elements = array
                .elements
                .iter()
                .map(|item| apply_single_transform_op(item, op))
                .collect();
            JsonValue::Array(JsonArray::new(
                elements,
                array.is_sequence,
                array.outer_wrapper,
            ))
        }
        JsonValue::Object(JsonObject(entries)) => {
            let mut out = Vec::with_capacity(entries.len());
            for (key, entry) in entries {
                out.push((key.clone(), apply_single_transform_op(entry, op)));
            }
            JsonValue::Object(JsonObject(out))
        }
        _ => value.clone(),
    }
}

fn apply_transform_to_value(
    value: &JsonValue,
    update: &JsonObject,
    delete_keys: &[String],
) -> JsonValue {
    let JsonValue::Object(JsonObject(existing)) = value else {
        return value.clone();
    };

    let mut out = existing.clone();
    for key in delete_keys {
        out.retain(|(name, _)| name != key);
    }
    for (key, update_value) in &update.0 {
        out.retain(|(name, _)| name != key);
        out.push((key.clone(), update_value.clone()));
    }
    JsonValue::Object(JsonObject(out))
}
