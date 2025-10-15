use futures::future::BoxFuture;
use std::any::Any;
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
pub struct JsonObject(pub Vec<(String, JsonValue)>);

#[derive(Clone)]
pub enum JsonValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(JsonArray),
    Object(JsonObject),
    Function(JsonFunction),
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

#[derive(Clone)]
pub struct JsonFunction {
    callable: Arc<dyn JsonCallable>,
}

impl JsonFunction {
    pub fn new(callable: Arc<dyn JsonCallable>) -> Self {
        Self { callable }
    }

    pub fn call(
        &self,
        ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
        self.callable.call(ctx, args)
    }

    pub fn arity(&self) -> Option<usize> {
        self.callable.arity()
    }

    pub fn ptr_eq(&self, other: &JsonFunction) -> bool {
        Arc::ptr_eq(&self.callable, &other.callable)
    }
}

impl fmt::Debug for JsonFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JsonFunction")
            .field("callable", &"<opaque>")
            .finish()
    }
}

pub type CallbackHandle = Arc<dyn Any + Send + Sync>;

#[derive(Clone, Debug)]
pub struct JsonataFocus {
    pub input: JsonValue,
    pub handle: Option<CallbackHandle>,
}

impl JsonataFocus {
    pub fn new(input: JsonValue) -> Self {
        Self {
            input,
            handle: None,
        }
    }

    pub fn with_handle(input: JsonValue, handle: Option<CallbackHandle>) -> Self {
        Self { input, handle }
    }
}

impl PartialEq for JsonataFocus {
    fn eq(&self, other: &Self) -> bool {
        self.input == other.input
    }
}

#[derive(Clone, Default)]
pub struct FunctionContext {
    pub focus: Option<Arc<JsonataFocus>>,
}

impl FunctionContext {
    pub fn empty() -> Self {
        Self { focus: None }
    }

    pub fn with_focus(focus: JsonataFocus) -> Self {
        Self {
            focus: Some(Arc::new(focus)),
        }
    }

    pub fn focus(&self) -> Option<Arc<JsonataFocus>> {
        self.focus.clone()
    }
}

pub trait JsonCallable: Send + Sync {
    fn call(
        &self,
        ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, JsonError>>;

    fn arity(&self) -> Option<usize> {
        None
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

impl fmt::Debug for JsonValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonValue::Undefined => write!(f, "Undefined"),
            JsonValue::Null => write!(f, "Null"),
            JsonValue::Bool(v) => f.debug_tuple("Bool").field(v).finish(),
            JsonValue::Number(n) => f.debug_tuple("Number").field(n).finish(),
            JsonValue::String(s) => f.debug_tuple("String").field(s).finish(),
            JsonValue::Array(a) => f.debug_tuple("Array").field(a).finish(),
            JsonValue::Object(o) => f.debug_tuple("Object").field(o).finish(),
            JsonValue::Function(func) => f.debug_tuple("Function").field(func).finish(),
        }
    }
}

impl PartialEq for JsonValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (JsonValue::Undefined, JsonValue::Undefined) => true,
            (JsonValue::Null, JsonValue::Null) => true,
            (JsonValue::Bool(a), JsonValue::Bool(b)) => a == b,
            (JsonValue::Number(a), JsonValue::Number(b)) => a == b,
            (JsonValue::String(a), JsonValue::String(b)) => a == b,
            (JsonValue::Array(a), JsonValue::Array(b)) => a == b,
            (JsonValue::Object(a), JsonValue::Object(b)) => a == b,
            (JsonValue::Function(a), JsonValue::Function(b)) => a.ptr_eq(b),
            _ => false,
        }
    }
}
