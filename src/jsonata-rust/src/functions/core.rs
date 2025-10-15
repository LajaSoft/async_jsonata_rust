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
                JsonValue::Array(JsonArray::new(results, true, false))
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

    JsonValue::Array(JsonArray::new(combined, true, false))
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
            JsonValue::Array(JsonArray::new(ordered, true, false))
        }
        JsonValue::Object(JsonObject(props)) => {
            let mut ordered = Vec::with_capacity(props.len());
            for (name, _) in props {
                ordered.push(JsonValue::String(name.clone()));
            }
            JsonValue::Array(JsonArray::new(ordered, true, false))
        }
        _ => JsonValue::Array(JsonArray::empty_sequence()),
    }
}

fn boolean_internal(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Undefined => JsonValue::Undefined,
        JsonValue::Null => JsonValue::Bool(false),
        JsonValue::Bool(flag) => JsonValue::Bool(*flag),
        JsonValue::Number(num) => JsonValue::Bool(*num != 0.0),
        JsonValue::String(text) => JsonValue::Bool(!text.is_empty()),
        JsonValue::Array(array) => match array.elements.len() {
            0 => JsonValue::Bool(false),
            1 => boolean_internal(&array.elements[0]),
            _ => {
                let mut truthy = false;
                for element in &array.elements {
                    if matches!(boolean_internal(element), JsonValue::Bool(true)) {
                        truthy = true;
                        break;
                    }
                }
                JsonValue::Bool(truthy)
            }
        },
        JsonValue::Object(JsonObject(props)) => JsonValue::Bool(!props.is_empty()),
    }
}

pub fn boolean(value: &JsonValue) -> JsonValue {
    boolean_internal(value)
}

pub fn not(value: &JsonValue) -> JsonValue {
    match boolean_internal(value) {
        JsonValue::Undefined => JsonValue::Undefined,
        JsonValue::Bool(flag) => JsonValue::Bool(!flag),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boolean_handles_primitives() {
        assert_eq!(boolean(&JsonValue::Undefined), JsonValue::Undefined);
        assert_eq!(boolean(&JsonValue::Null), JsonValue::Bool(false));
        assert_eq!(boolean(&JsonValue::Bool(true)), JsonValue::Bool(true));
        assert_eq!(boolean(&JsonValue::Number(0.0)), JsonValue::Bool(false));
        assert_eq!(boolean(&JsonValue::Number(42.0)), JsonValue::Bool(true));
        assert_eq!(
            boolean(&JsonValue::String(String::from(""))),
            JsonValue::Bool(false)
        );
        assert_eq!(
            boolean(&JsonValue::String(String::from("x"))),
            JsonValue::Bool(true)
        );
    }

    #[test]
    fn boolean_handles_arrays() {
        let empty = JsonValue::Array(JsonArray::empty_sequence());
        assert_eq!(boolean(&empty), JsonValue::Bool(false));

        let single_undefined =
            JsonValue::Array(JsonArray::new(vec![JsonValue::Undefined], true, false));
        assert_eq!(boolean(&single_undefined), JsonValue::Undefined);

        let nested = JsonValue::Array(JsonArray::new(
            vec![
                JsonValue::Bool(false),
                JsonValue::Bool(true),
                JsonValue::Bool(false),
            ],
            true,
            false,
        ));
        assert_eq!(boolean(&nested), JsonValue::Bool(true));
    }

    #[test]
    fn boolean_handles_objects() {
        let empty = JsonValue::Object(JsonObject(vec![]));
        assert_eq!(boolean(&empty), JsonValue::Bool(false));

        let non_empty = JsonValue::Object(JsonObject(vec![(
            "key".to_string(),
            JsonValue::Number(1.0),
        )]));
        assert_eq!(boolean(&non_empty), JsonValue::Bool(true));
    }

    #[test]
    fn not_inverts_boolean() {
        assert_eq!(not(&JsonValue::Bool(true)), JsonValue::Bool(false));
        assert_eq!(not(&JsonValue::Bool(false)), JsonValue::Bool(true));

        let undefined = JsonValue::Undefined;
        assert_eq!(not(&undefined), JsonValue::Undefined);
    }
}
