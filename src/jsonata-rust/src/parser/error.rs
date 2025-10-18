use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ParserError {
    pub code: String,
    pub position: usize,
    pub token: Option<Value>,
    pub value: Option<Value>,
    pub remaining: Option<Vec<Value>>,
}

impl ParserError {
    pub fn new<S: Into<String>>(code: S, position: usize) -> Self {
        Self {
            code: code.into(),
            position,
            token: None,
            value: None,
            remaining: None,
        }
    }

    pub fn with_token(mut self, token: Value) -> Self {
        self.token = Some(token);
        self
    }

    pub fn with_value(mut self, value: Value) -> Self {
        self.value = Some(value);
        self
    }

    pub fn with_remaining(mut self, remaining: Vec<Value>) -> Self {
        self.remaining = Some(remaining);
        self
    }
}
