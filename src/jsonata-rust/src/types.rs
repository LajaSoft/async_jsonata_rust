use std::fmt;

#[derive(Clone, Debug)]
pub struct JsonArray {
    pub elements: Vec<JsonValue>,
    pub is_sequence: bool,
    pub outer_wrapper: bool,
}

impl JsonArray {
    pub fn new(elements: Vec<JsonValue>, is_sequence: bool, outer_wrapper: bool) -> Self {
        Self {
            elements,
            is_sequence,
            outer_wrapper,
        }
    }

    pub fn empty_sequence() -> Self {
        Self {
            elements: Vec::new(),
            is_sequence: true,
            outer_wrapper: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct JsonObject(pub Vec<(String, JsonValue)>);

#[derive(Clone, Debug)]
pub enum JsonValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(JsonArray),
    Object(JsonObject),
}

impl JsonValue {
    pub fn undefined() -> Self {
        JsonValue::Undefined
    }

    pub fn bool(value: bool) -> Self {
        JsonValue::Bool(value)
    }

    pub fn string(value: impl Into<String>) -> Self {
        JsonValue::String(value.into())
    }

    pub fn sequence(elements: Vec<JsonValue>) -> Self {
        JsonValue::Array(JsonArray::new(elements, true, false))
    }

    pub fn is_undefined(&self) -> bool {
        matches!(self, JsonValue::Undefined)
    }
}

#[derive(Debug, Clone)]
pub struct JsonError {
    pub code: &'static str,
    pub message: String,
}

impl JsonError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for JsonError {}
