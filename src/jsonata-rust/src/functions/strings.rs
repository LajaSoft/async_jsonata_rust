use serde_json::Value;

/// Mimics the JSONata `$string()` behaviour for the subset of cases we support so far.
pub fn stringify(value: Option<&Value>, prettify: bool) -> Option<String> {
    let value = value?;

    if let Some(str_value) = value.as_str() {
        return Some(str_value.to_owned());
    }

    if prettify {
        serde_json::to_string_pretty(value).ok()
    } else {
        serde_json::to_string(value).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stringify_none() {
        assert_eq!(stringify(None, false), None);
    }

    #[test]
    fn stringify_string_passthrough() {
        let value = Value::String("hello".to_owned());
        assert_eq!(stringify(Some(&value), false), Some("hello".to_owned()));
    }

    #[test]
    fn stringify_number() {
        let value = Value::Number(42.into());
        assert_eq!(stringify(Some(&value), false), Some("42".to_owned()));
    }

    #[test]
    fn stringify_pretty_object() {
        let value = serde_json::json!({ "foo": "bar" });
        let pretty = stringify(Some(&value), true).unwrap();
        assert!(pretty.contains("\n"));
    }
}
