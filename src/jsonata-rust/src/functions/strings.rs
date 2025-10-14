use crate::types::{JsonArray, JsonError, JsonObject, JsonValue};
use serde_json::{Map, Number, Value};

fn to_serde_value(value: &JsonValue) -> Value {
    match value {
        JsonValue::Undefined => Value::Null,
        JsonValue::Null => Value::Null,
        JsonValue::Bool(flag) => Value::Bool(*flag),
        JsonValue::Number(num) => Number::from_f64(*num)
            .map(Value::Number)
            .unwrap_or(Value::Null),
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

fn to_number(value: &JsonValue) -> Option<f64> {
    match value {
        JsonValue::Undefined => None,
        JsonValue::Null => Some(0.0),
        JsonValue::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        JsonValue::Number(num) => Some(*num),
        JsonValue::String(text) => text.parse::<f64>().ok(),
        JsonValue::Array(_) | JsonValue::Object(_) => None,
    }
}

fn to_integer(value: &JsonValue) -> Option<i64> {
    to_number(value).map(|num| num.trunc() as i64)
}

fn ensure_string(value: &JsonValue, prettify: bool) -> Result<Option<String>, JsonError> {
    match value {
        JsonValue::Undefined => Ok(None),
        JsonValue::Null => Ok(Some("null".to_owned())),
        JsonValue::Bool(flag) => Ok(Some(flag.to_string())),
        JsonValue::Number(num) => {
            if !num.is_finite() {
                Err(JsonError::new(
                    "D3001",
                    format!("Unable to represent number {}", num),
                ))
            } else {
                Ok(Some(num.to_string()))
            }
        }
        JsonValue::String(text) => Ok(Some(text.clone())),
        JsonValue::Array(JsonArray {
            elements,
            outer_wrapper,
            ..
        }) if *outer_wrapper => {
            if let Some(first) = elements.first() {
                ensure_string(first, prettify)
            } else {
                Ok(None)
            }
        }
        _ => match string(value, prettify)? {
            JsonValue::Undefined => Ok(None),
            JsonValue::String(text) => Ok(Some(text)),
            _ => Err(JsonError::new("D3137", "Unable to convert value to string")),
        },
    }
}

fn slice_chars(chars: &[char], start: i64, length: Option<i64>) -> String {
    let len = chars.len() as i64;
    let mut start_idx = start;
    if len + start_idx < 0 {
        start_idx = 0;
    }

    let take = match length {
        Some(len_arg) if len_arg <= 0 => return String::new(),
        Some(len_arg) => {
            if start_idx >= 0 {
                (start_idx + len_arg).min(len)
            } else {
                (len + start_idx + len_arg).min(len)
            }
        }
        None => len,
    };

    let start_resolved = if start_idx >= 0 {
        start_idx.min(len)
    } else {
        (len + start_idx).max(0)
    };

    let end_resolved = take.max(start_resolved).min(len);

    chars[start_resolved as usize..end_resolved as usize]
        .iter()
        .collect::<String>()
}

pub fn string(value: &JsonValue, prettify: bool) -> Result<JsonValue, JsonError> {
    match value {
        JsonValue::Undefined => return Ok(JsonValue::Undefined),
        JsonValue::String(text) => return Ok(JsonValue::String(text.clone())),
        JsonValue::Number(num) => {
            if !num.is_finite() {
                return Err(JsonError::new(
                    "D3001",
                    format!("Unable to represent number {}", num),
                ));
            }
        }
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
        JsonValue::Null => Ok(JsonValue::String("null".to_owned())),
        JsonValue::Bool(flag) => Ok(JsonValue::String(flag.to_string())),
        JsonValue::Number(num) => Ok(JsonValue::String(num.to_string())),
        JsonValue::String(text) => Ok(JsonValue::String(text.clone())),
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

pub fn substring(
    value: &JsonValue,
    start: &JsonValue,
    length: &JsonValue,
) -> Result<JsonValue, JsonError> {
    let string_value = match ensure_string(value, false)? {
        Some(str_val) => str_val,
        None => return Ok(JsonValue::Undefined),
    };

    let chars: Vec<char> = string_value.chars().collect();
    let length_idx = to_integer(length);
    let mut start_idx = to_integer(start).unwrap_or(0);
    if chars.len() as i64 + start_idx < 0 {
        start_idx = 0;
    }
    let result = slice_chars(&chars, start_idx, length_idx);
    Ok(JsonValue::String(result))
}

pub fn substring_before(value: &JsonValue, chars: &JsonValue) -> Result<JsonValue, JsonError> {
    let string_value = match ensure_string(value, false)? {
        Some(str_val) => str_val,
        None => return Ok(JsonValue::Undefined),
    };
    let chars_value = ensure_string(chars, false)?.unwrap_or_else(|| "undefined".to_owned());

    if let Some(pos) = string_value.find(&chars_value) {
        Ok(JsonValue::String(string_value[..pos].to_owned()))
    } else {
        Ok(JsonValue::String(string_value))
    }
}

pub fn substring_after(value: &JsonValue, chars: &JsonValue) -> Result<JsonValue, JsonError> {
    let string_value = match ensure_string(value, false)? {
        Some(str_val) => str_val,
        None => return Ok(JsonValue::Undefined),
    };
    let chars_value = ensure_string(chars, false)?.unwrap_or_else(|| "undefined".to_owned());

    if let Some(pos) = string_value.find(&chars_value) {
        Ok(JsonValue::String(
            string_value[pos + chars_value.len()..].to_owned(),
        ))
    } else {
        Ok(JsonValue::String(string_value))
    }
}

pub fn lowercase(value: &JsonValue) -> Result<JsonValue, JsonError> {
    let string_value = match ensure_string(value, false)? {
        Some(str_val) => str_val,
        None => return Ok(JsonValue::Undefined),
    };
    Ok(JsonValue::String(string_value.to_lowercase()))
}

pub fn uppercase(value: &JsonValue) -> Result<JsonValue, JsonError> {
    let string_value = match ensure_string(value, false)? {
        Some(str_val) => str_val,
        None => return Ok(JsonValue::Undefined),
    };
    Ok(JsonValue::String(string_value.to_uppercase()))
}

pub fn length(value: &JsonValue) -> Result<JsonValue, JsonError> {
    let string_value = match ensure_string(value, false)? {
        Some(str_val) => str_val,
        None => return Ok(JsonValue::Undefined),
    };
    let count = string_value.chars().count() as f64;
    Ok(JsonValue::Number(count))
}

pub fn trim(value: &JsonValue) -> Result<JsonValue, JsonError> {
    let string_value = match ensure_string(value, false)? {
        Some(str_val) => str_val,
        None => return Ok(JsonValue::Undefined),
    };
    let normalized = string_value
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ");
    Ok(JsonValue::String(normalized))
}

pub fn pad(
    value: &JsonValue,
    width: &JsonValue,
    char_value: &JsonValue,
) -> Result<JsonValue, JsonError> {
    let string_value = match ensure_string(value, false)? {
        Some(str_val) => str_val,
        None => return Ok(JsonValue::Undefined),
    };

    let width_num = to_integer(width).unwrap_or(0);
    if width_num == 0 {
        return Ok(JsonValue::String(string_value));
    }

    let pad_char = ensure_string(char_value, false)?
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| " ".to_owned());

    let current_len = string_value.chars().count();
    let target_len = width_num.abs() as usize;
    if target_len <= current_len {
        return Ok(JsonValue::String(string_value));
    }

    let pad_length = target_len - current_len;
    let mut padding = String::new();
    while padding.chars().count() < pad_length {
        padding.push_str(&pad_char);
    }
    let padding: String = padding.chars().take(pad_length).collect();

    let result = if width_num > 0 {
        format!("{}{}", string_value, padding)
    } else {
        format!("{}{}", padding, string_value)
    };

    Ok(JsonValue::String(result))
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
    fn substring_basic() {
        let value = JsonValue::String("Hello".to_owned());
        let start = JsonValue::Number(1.0);
        let length = JsonValue::Number(2.0);
        let result = substring(&value, &start, &length).unwrap();
        assert!(matches!(result, JsonValue::String(ref s) if s == "el"));
    }

    #[test]
    fn pad_left() {
        let value = JsonValue::String("7".to_owned());
        let width = JsonValue::Number(-3.0);
        let pad_char = JsonValue::String("0".to_owned());
        let result = pad(&value, &width, &pad_char).unwrap();
        assert!(matches!(result, JsonValue::String(ref s) if s == "007"));
    }
}
