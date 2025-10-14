use std::collections::HashSet;

use crate::types::{JsonArray, JsonObject, JsonValue};

pub fn lookup(input: &JsonValue, key: &str) -> JsonValue {
    match input {
        JsonValue::Undefined => JsonValue::Undefined,
        JsonValue::Array(array) => {
            let mut results: Vec<JsonValue> = Vec::new();
            for item in &array.elements {
                let resolved = lookup(item, key);
                match resolved {
                    JsonValue::Undefined => {}
                    JsonValue::Array(seq) => {
                        for element in seq.elements {
                            results.push(element);
                        }
                    }
                    value => results.push(value),
                }
            }
            if results.is_empty() {
                JsonValue::Undefined
            } else {
                JsonValue::Array(JsonArray::new(results, true))
            }
        }
        JsonValue::Object(JsonObject(props)) => {
            for (prop_key, value) in props {
                if prop_key == key {
                    return value.clone();
                }
            }
            JsonValue::Undefined
        }
        _ => JsonValue::Undefined,
    }
}

pub fn append(left: &JsonValue, right: &JsonValue) -> JsonValue {
    if left.is_undefined() {
        return right.clone();
    }
    if right.is_undefined() {
        return left.clone();
    }

    let mut combined: Vec<JsonValue> = match left {
        JsonValue::Array(array) => array.elements.clone(),
        value => vec![value.clone()],
    };

    match right {
        JsonValue::Array(array) => combined.extend(array.elements.clone()),
        value => combined.push(value.clone()),
    }

    JsonValue::Array(JsonArray::new(combined, true))
}

pub fn exists(value: &JsonValue) -> JsonValue {
    JsonValue::Bool(!value.is_undefined())
}

pub fn keys(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Undefined => JsonValue::Array(JsonArray::empty_sequence()),
        JsonValue::Array(array) => {
            let mut seen = HashSet::new();
            let mut ordered: Vec<JsonValue> = Vec::new();
            for item in &array.elements {
                if let JsonValue::Array(seq) = keys(item) {
                    for element in seq.elements {
                        if let JsonValue::String(key) = element {
                            if seen.insert(key.clone()) {
                                ordered.push(JsonValue::String(key));
                            }
                        }
                    }
                }
            }
            JsonValue::Array(JsonArray::new(ordered, true))
        }
        JsonValue::Object(JsonObject(props)) => {
            let mut ordered = Vec::with_capacity(props.len());
            for (name, _) in props {
                ordered.push(JsonValue::String(name.clone()));
            }
            JsonValue::Array(JsonArray::new(ordered, true))
        }
        _ => JsonValue::Array(JsonArray::empty_sequence()),
    }
}
