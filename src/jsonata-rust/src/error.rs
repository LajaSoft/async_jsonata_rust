use std::collections::BTreeMap;
use std::fmt;

use serde_json::Value;

use crate::parser::ParserError;
use crate::types::JsonError;

/// Unified crate error used by stable public APIs.
///
/// # Examples
/// ```rust
/// let err = async_jsonata_rust::Error::new("D3040", "Sqrt domain error");
/// assert_eq!(err.code(), "D3040");
/// ```
#[derive(Debug, Clone)]
pub struct Error {
    code: String,
    message: String,
    position: Option<usize>,
    token: Option<Value>,
    value: Option<Value>,
    remaining: Option<Vec<Value>>,
    context: BTreeMap<String, Value>,
}

impl Error {
    /// Creates a new error with JSONata-compatible code.
    ///
    /// # Examples
    /// ```rust
    /// let err = async_jsonata_rust::Error::new("S0201", "Unexpected token");
    /// assert_eq!(err.code(), "S0201");
    /// ```
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            position: None,
            token: None,
            value: None,
            remaining: None,
            context: BTreeMap::new(),
        }
    }

    /// Creates a parser error from parser internals.
    ///
    /// # Examples
    /// ```rust
    /// let parse_error = async_jsonata_rust::parse_expression("1+", false).unwrap_err();
    /// let err = async_jsonata_rust::Error::from(parse_error);
    /// assert!(!err.code().is_empty());
    /// ```
    pub fn parser(err: ParserError) -> Self {
        let mut out = Self::new(err.code, "parser error");
        out.position = Some(err.position);
        out.token = err.token;
        out.value = err.value;
        out.remaining = err.remaining;
        out
    }

    /// Creates a runtime error from evaluator/function internals.
    ///
    /// # Examples
    /// ```rust
    /// let runtime = async_jsonata_rust::JsonError::new("D3040", "Sqrt domain error");
    /// let err = async_jsonata_rust::Error::from(runtime);
    /// assert_eq!(err.code(), "D3040");
    /// ```
    pub fn runtime(err: JsonError) -> Self {
        Self::new(err.code, err.message)
    }

    /// Creates a not-implemented error for incomplete features.
    ///
    /// # Examples
    /// ```rust
    /// let err = async_jsonata_rust::Error::not_implemented("evaluator pending");
    /// assert_eq!(err.code(), "E0001");
    /// ```
    pub fn not_implemented(message: impl Into<String>) -> Self {
        Self::new("E0001", message)
    }

    /// Adds named context field.
    ///
    /// # Examples
    /// ```rust
    /// let err = async_jsonata_rust::Error::new("E1", "oops")
    ///     .with_context("field", serde_json::Value::String("value".into()));
    /// assert!(err.context().contains_key("field"));
    /// ```
    pub fn with_context(mut self, key: impl Into<String>, value: Value) -> Self {
        self.context.insert(key.into(), value);
        self
    }

    /// Returns JSONata error code (`D3040`, `S0201`, ...).
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns human-readable message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns parser position if known.
    pub fn position(&self) -> Option<usize> {
        self.position
    }

    /// Returns token payload if known.
    pub fn token(&self) -> Option<&Value> {
        self.token.as_ref()
    }

    /// Returns parser value payload if known.
    pub fn value(&self) -> Option<&Value> {
        self.value.as_ref()
    }

    /// Returns unparsed trailing token payload if known.
    pub fn remaining(&self) -> Option<&[Value]> {
        self.remaining.as_deref()
    }

    /// Returns structured context fields.
    pub fn context(&self) -> &BTreeMap<String, Value> {
        &self.context
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}

impl From<ParserError> for Error {
    fn from(value: ParserError) -> Self {
        Self::parser(value)
    }
}

impl From<JsonError> for Error {
    fn from(value: JsonError) -> Self {
        Self::runtime(value)
    }
}
