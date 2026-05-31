use serde_json::Value;

use crate::types::{JsonArray, JsonObject, JsonValue};

pub(super) fn object_keys_from_value(value: &JsonValue) -> Vec<String> {
    match value {
        JsonValue::Undefined => Vec::new(),
        JsonValue::String(text) => vec![text.clone()],
        JsonValue::Number(num) => vec![num.to_string()],
        JsonValue::Bool(flag) => vec![flag.to_string()],
        JsonValue::Null => vec!["null".to_owned()],
        JsonValue::Array(array) => {
            let mut keys = Vec::new();
            for item in &array.elements {
                keys.extend(object_keys_from_value(item));
            }
            keys
        }
        JsonValue::Object(_) | JsonValue::Function(_) => Vec::new(),
    }
}

pub(super) fn upsert_object_property(object: &mut JsonObject, key: String, value: JsonValue) {
    for (existing_key, existing_value) in &mut object.0 {
        if *existing_key == key {
            *existing_value = value;
            return;
        }
    }
    object.0.push((key, value));
}

pub(super) fn materialize_value(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Array(array) => {
            let elements = array.elements.iter().map(materialize_value).collect();
            JsonValue::Array(JsonArray::new(elements, false, array.outer_wrapper))
        }
        JsonValue::Object(JsonObject(entries)) => {
            let mut out = Vec::with_capacity(entries.len());
            for (key, item) in entries {
                out.push((key.clone(), materialize_value(item)));
            }
            JsonValue::Object(JsonObject(out))
        }
        other => other.clone(),
    }
}

pub(super) fn json_value_from_serde(value: &Value) -> JsonValue {
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
        Value::Object(map) => {
            // The tokenizer encodes the `undefined` literal as a sentinel object;
            // a `value` AST node carrying it evaluates to JSONata `undefined`.
            if map.len() == 1
                && map
                    .get("__jsonata_undefined__")
                    .and_then(Value::as_bool)
                    == Some(true)
            {
                return JsonValue::Undefined;
            }
            JsonValue::Object(JsonObject(
                map.iter()
                    .map(|(key, item)| (key.clone(), json_value_from_serde(item)))
                    .collect(),
            ))
        }
    }
}
