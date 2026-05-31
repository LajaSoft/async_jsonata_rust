use futures::future::BoxFuture;
use serde_json::{Number, Value};
use std::any::Any;
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct JsonataArray {
    pub elements: Vec<JsonataValue>,
    pub is_sequence: bool,
    pub outer_wrapper: bool,
}

impl JsonataArray {
    pub fn new(elements: Vec<JsonataValue>, is_sequence: bool, outer_wrapper: bool) -> Self {
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
pub struct JsonataObject(pub Vec<(String, JsonataValue)>);

// Оставляем старые типы для совместимости пока не переделаем всё
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

#[derive(Clone)]
pub enum JsonataValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(JsonataArray),
    Object(JsonataObject),
    Function(JsonataFunction),
    NativeRef(NativeRef),
}

// Хранит napi ссылку на JS объект
#[derive(Clone)]
pub struct NativeRef {
    // Здесь будет храниться napi reference
    pub handle: Arc<dyn Any + Send + Sync>,
    pub value_type: NativeType,
}

#[derive(Clone, Debug)]
pub enum NativeType {
    JsFunction,
    JsObject,
    JsArray,
    JsOther,
}

impl JsonataValue {
    pub fn undefined() -> Self {
        JsonataValue::Undefined
    }

    pub fn bool(value: bool) -> Self {
        JsonataValue::Bool(value)
    }

    pub fn string(value: impl Into<String>) -> Self {
        JsonataValue::String(value.into())
    }

    pub fn sequence(elements: Vec<JsonataValue>) -> Self {
        JsonataValue::Array(JsonataArray::new(elements, true, false))
    }

    pub fn is_undefined(&self) -> bool {
        matches!(self, JsonataValue::Undefined)
    }

    pub fn native_ref(handle: Arc<dyn Any + Send + Sync>, value_type: NativeType) -> Self {
        JsonataValue::NativeRef(NativeRef { handle, value_type })
    }
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

    /// Builds a [`JsonValue`] from a `serde_json::Value`.
    ///
    /// Plain JSON has no notion of "undefined" or functions, so the result only
    /// uses the data-bearing variants. Arrays are materialized as non-sequence,
    /// non-wrapper arrays.
    pub fn from_serde_json(value: &Value) -> Self {
        match value {
            Value::Null => JsonValue::Null,
            Value::Bool(flag) => JsonValue::Bool(*flag),
            Value::Number(num) => JsonValue::Number(num.as_f64().unwrap_or(0.0)),
            Value::String(text) => JsonValue::String(text.clone()),
            Value::Array(values) => JsonValue::Array(JsonArray::new(
                values.iter().map(JsonValue::from_serde_json).collect(),
                false,
                false,
            )),
            Value::Object(map) => JsonValue::Object(JsonObject(
                map.iter()
                    .map(|(key, item)| (key.clone(), JsonValue::from_serde_json(item)))
                    .collect(),
            )),
        }
    }

    /// Converts a [`JsonValue`] into a `serde_json::Value`.
    ///
    /// Returns `None` for values that have no JSON representation, namely
    /// [`JsonValue::Undefined`] and [`JsonValue::Function`]. Following JSONata
    /// sequence semantics, `undefined` array elements and object values are
    /// dropped. Whole-number floats serialize as JSON integers so they compare
    /// equal to integer literals in the official test suite.
    pub fn to_serde_json(&self) -> Option<Value> {
        match self {
            JsonValue::Undefined | JsonValue::Function(_) => None,
            JsonValue::Null => Some(Value::Null),
            JsonValue::Bool(flag) => Some(Value::Bool(*flag)),
            JsonValue::Number(num) => Some(Value::Number(number_to_json(*num)?)),
            JsonValue::String(text) => Some(Value::String(text.clone())),
            JsonValue::Array(array) => {
                let items = array
                    .elements
                    .iter()
                    .filter_map(JsonValue::to_serde_json)
                    .collect();
                Some(Value::Array(items))
            }
            JsonValue::Object(JsonObject(entries)) => {
                let mut map = serde_json::Map::with_capacity(entries.len());
                for (key, item) in entries {
                    if let Some(json) = item.to_serde_json() {
                        map.insert(key.clone(), json);
                    }
                }
                Some(Value::Object(map))
            }
        }
    }
}

/// Encodes an `f64` as a `serde_json::Number`, using an integer representation
/// for whole numbers so that `5.0` compares equal to a JSON `5`.
fn number_to_json(num: f64) -> Option<Number> {
    if !num.is_finite() {
        return None;
    }
    if num.fract() == 0.0 && num >= i64::MIN as f64 && num <= i64::MAX as f64 {
        return Some(Number::from(num as i64));
    }
    Number::from_f64(num)
}

#[derive(Clone)]
pub struct JsonataFunction {
    callable: Arc<dyn JsonataCallable>,
}

impl JsonataFunction {
    pub fn new(callable: Arc<dyn JsonataCallable>) -> Self {
        Self { callable }
    }

    pub fn call(
        &self,
        ctx: FunctionContext,
        args: Vec<JsonataValue>,
    ) -> BoxFuture<'static, Result<JsonataValue, JsonError>> {
        self.callable.call(ctx, args)
    }

    pub fn arity(&self) -> Option<usize> {
        self.callable.arity()
    }

    pub fn ptr_eq(&self, other: &JsonataFunction) -> bool {
        Arc::ptr_eq(&self.callable, &other.callable)
    }

    pub fn as_callable(&self) -> &dyn JsonataCallable {
        &*self.callable
    }
}

impl fmt::Debug for JsonataFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JsonataFunction")
            .field("callable", &"<opaque>")
            .finish()
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

    pub fn as_callable(&self) -> &dyn JsonCallable {
        &*self.callable
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

pub trait JsonataCallable: Send + Sync + Any {
    fn call(
        &self,
        ctx: FunctionContext,
        args: Vec<JsonataValue>,
    ) -> BoxFuture<'static, Result<JsonataValue, JsonError>>;

    fn arity(&self) -> Option<usize> {
        None
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync);
}

pub trait JsonCallable: Send + Sync + Any {
    fn call(
        &self,
        ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, JsonError>>;

    fn arity(&self) -> Option<usize> {
        None
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync);
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

impl fmt::Debug for JsonataValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonataValue::Undefined => write!(f, "Undefined"),
            JsonataValue::Null => write!(f, "Null"),
            JsonataValue::Bool(v) => f.debug_tuple("Bool").field(v).finish(),
            JsonataValue::Number(n) => f.debug_tuple("Number").field(n).finish(),
            JsonataValue::String(s) => f.debug_tuple("String").field(s).finish(),
            JsonataValue::Array(a) => f.debug_tuple("Array").field(a).finish(),
            JsonataValue::Object(o) => f.debug_tuple("Object").field(o).finish(),
            JsonataValue::Function(func) => f.debug_tuple("Function").field(func).finish(),
            JsonataValue::NativeRef(nr) => {
                f.debug_tuple("NativeRef").field(&nr.value_type).finish()
            }
        }
    }
}

impl PartialEq for JsonataValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (JsonataValue::Undefined, JsonataValue::Undefined) => true,
            (JsonataValue::Null, JsonataValue::Null) => true,
            (JsonataValue::Bool(a), JsonataValue::Bool(b)) => a == b,
            (JsonataValue::Number(a), JsonataValue::Number(b)) => a == b,
            (JsonataValue::String(a), JsonataValue::String(b)) => a == b,
            (JsonataValue::Array(a), JsonataValue::Array(b)) => a == b,
            (JsonataValue::Object(a), JsonataValue::Object(b)) => a == b,
            (JsonataValue::Function(a), JsonataValue::Function(b)) => a.ptr_eq(b),
            (JsonataValue::NativeRef(_), JsonataValue::NativeRef(_)) => false, // Native refs не сравниваем
            _ => false,
        }
    }
}

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
