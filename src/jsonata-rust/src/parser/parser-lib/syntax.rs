use std::collections::HashMap;

use serde_json::{json, Value};

use super::super::tokenizer::TokenValue;

pub(crate) fn token_value_to_json(value: &TokenValue) -> Value {
    match value {
        TokenValue::None | TokenValue::Undefined | TokenValue::Null => Value::Null,
        TokenValue::Number(n) => json!(n),
        TokenValue::String(s) => Value::String(s.clone()),
        TokenValue::Regex { pattern, flags } => json!({ "pattern": pattern, "flags": flags }),
        TokenValue::Boolean(b) => Value::Bool(*b),
    }
}

pub(crate) fn operator_table() -> HashMap<String, u32> {
    let mut map = HashMap::new();
    map.insert(".".to_string(), 75);
    map.insert("[".to_string(), 80);
    map.insert("]".to_string(), 0);
    map.insert("{".to_string(), 70);
    map.insert("}".to_string(), 0);
    map.insert("(".to_string(), 80);
    map.insert(")".to_string(), 0);
    map.insert(",".to_string(), 0);
    map.insert("@".to_string(), 80);
    map.insert("#".to_string(), 80);
    map.insert(";".to_string(), 0);
    map.insert(":".to_string(), 0);
    map.insert("?".to_string(), 20);
    map.insert("+".to_string(), 50);
    map.insert("-".to_string(), 50);
    map.insert("*".to_string(), 60);
    map.insert("/".to_string(), 60);
    map.insert("%".to_string(), 60);
    map.insert("|".to_string(), 0);
    map.insert("=".to_string(), 40);
    map.insert("<".to_string(), 40);
    map.insert(">".to_string(), 40);
    map.insert("^".to_string(), 40);
    map.insert("**".to_string(), 60);
    map.insert("..".to_string(), 0);
    map.insert(":=".to_string(), 10);
    map.insert("!=".to_string(), 40);
    map.insert("<=".to_string(), 40);
    map.insert(">=".to_string(), 40);
    map.insert("~>".to_string(), 40);
    map.insert("?:".to_string(), 40);
    map.insert("??".to_string(), 40);
    map.insert("and".to_string(), 30);
    map.insert("or".to_string(), 25);
    map.insert("in".to_string(), 40);
    map.insert("&".to_string(), 50);
    map
}
