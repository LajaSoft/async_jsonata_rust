use crate::types::{JsonArray, JsonError, JsonObject, JsonValue};
use serde_json::{Map, Number, Value};

fn to_serde_value(value: &JsonValue) -> Value {
    match value {
        JsonValue::Undefined => Value::Null,
        JsonValue::Null => Value::Null,
        JsonValue::Bool(flag) => Value::Bool(*flag),
        JsonValue::Number(num) => {
            if let Some(number) = Number::from_f64(*num) {
                Value::Number(number)
            } else {
                Value::Null
            }
        }
        JsonValue::String(text) => Value::String(text.clone()),
        JsonValue::Array(JsonArray { elements, .. }) => {
            Value::Array(elements.iter().map(to_serde_value).collect())
        }
        JsonValue::Object(JsonObject(entries)) => {
            let mut map = Map::new();
            for (key, entry_value) in entries {
                map.insert(key.clone(), to_serde_value(entry_value));
            }
            Value::Object(map)
        }
    }
}

pub fn string(value: &JsonValue, prettify: bool) -> Result<JsonValue, JsonError> {
    match value {
        JsonValue::Undefined => return Ok(JsonValue::Undefined),
        JsonValue::String(text) => return Ok(JsonValue::String(text.clone())),
        _ => {}
    }

    let mut target = value;

    if let JsonValue::Array(JsonArray {
        elements,
        outer_wrapper,
        ..
    }) = value
    {
        if *outer_wrapper {
            if let Some(first) = elements.first() {
                target = first;
            } else {
                return Ok(JsonValue::Undefined);
            }
        }
    }

    match target {
        JsonValue::Undefined => Ok(JsonValue::Undefined),
        JsonValue::String(text) => Ok(JsonValue::String(text.clone())),
        JsonValue::Number(num) => {
            if !num.is_finite() {
                Err(JsonError::new(
                    "D3001",
                    format!("Unable to represent number {}", num),
                ))
            } else if prettify {
                Ok(JsonValue::String(num.to_string()))
            } else {
                Ok(JsonValue::String(num.to_string()))
            }
        }
        JsonValue::Bool(flag) => Ok(JsonValue::String(flag.to_string())),
        JsonValue::Null => Ok(JsonValue::String("null".to_owned())),
        JsonValue::Array(_) | JsonValue::Object(_) => {
            let serde_value = to_serde_value(target);
            let result = if prettify {
                serde_json::to_string_pretty(&serde_value)
            } else {
                serde_json::to_string(&serde_value)
            }
            .map_err(|err| JsonError::new("D3137", err.to_string()))?;
            Ok(JsonValue::String(result))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_undefined() {
        assert!(matches!(
            string(&JsonValue::Undefined, false).unwrap(),
            JsonValue::Undefined
        ));
    }

    #[test]
    fn string_passthrough() {
        let value = JsonValue::String("hello".to_owned());
        let result = string(&value, false).unwrap();
        assert!(matches!(result, JsonValue::String(ref s) if s == "hello"));
    }

    #[test]
    fn string_number() {
        let value = JsonValue::Number(42.0);
        let result = string(&value, false).unwrap();
        assert!(matches!(result, JsonValue::String(ref s) if s == "42"));
    }

    #[test]
    fn string_pretty_object() {
        let value = JsonValue::Object(JsonObject(vec![(
            "foo".to_owned(),
            JsonValue::String("bar".to_owned()),
        )]));
        let result = string(&value, true).unwrap();
        if let JsonValue::String(text) = result {
            assert!(text.contains('\n'));
        } else {
            panic!("expected string value");
        }
    }
}
