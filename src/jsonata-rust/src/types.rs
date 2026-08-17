use futures::future::BoxFuture;
use serde_json::{Number, Value};
use std::any::Any;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
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

    /// Calls this function and then drives any tail-call thunk it returns to a
    /// final value, iteratively (O(1) native stack). This is the single
    /// trampoline for tail-call optimisation: a lambda's `call` returns the raw
    /// tail thunk without recursing, and callers that need the actual result
    /// (function-call sites and higher-order built-ins like `$map`/`$reduce`)
    /// funnel through here so a deep — or infinite — tail recursion neither grows
    /// the stack nor is mistaken for a function-valued result.
    pub fn call_forced(
        &self,
        ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
        let this = self.clone();
        // The focus to drive thunks with; thunks capture their own focus, so this
        // only feeds signature/context validation, which thunks do not use.
        let focus = ctx.focus().map(|handle| handle.input.clone());
        // Enforce the current evaluation's (configurable) tail-step limit when a
        // budget is present; fall back to the compiled-in default only for
        // callables invoked outside an evaluation. `None` disables the backstop.
        let budget = ctx.budget().cloned();
        let max_tail_call_steps = match &budget {
            Some(budget) => budget.max_tail_call_steps(),
            None => Some(MAX_TAIL_CALL_STEPS),
        };
        Box::pin(async move {
            let mut value = this.call(ctx, args).await?;
            let mut steps = 0usize;
            loop {
                let thunk = match &value {
                    JsonValue::Function(function) if function.callable.is_thunk() => function.clone(),
                    _ => return Ok(value),
                };
                if let Some(max) = max_tail_call_steps {
                    if steps >= max {
                        return Err(JsonError::new(
                            "U1001",
                            "Stack overflow error: non-terminating recursive function call",
                        ));
                    }
                }
                let thunk_ctx = match &focus {
                    Some(input) => FunctionContext::with_focus(JsonataFocus::new(input.clone())),
                    None => FunctionContext::empty(),
                }
                .with_budget(budget.clone());
                value = thunk.call(thunk_ctx, Vec::new()).await?;
                steps += 1;
            }
        })
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
    /// The current evaluation's execution budget, propagated so the tail-call
    /// trampoline ([`JsonFunction::call_forced`]) enforces the configured
    /// (per-evaluation) step limit rather than only the compiled-in default.
    /// `None` for contexts built outside an evaluation.
    pub(crate) budget: Option<Arc<Budget>>,
}

impl FunctionContext {
    pub fn empty() -> Self {
        Self {
            focus: None,
            budget: None,
        }
    }

    pub fn with_focus(focus: JsonataFocus) -> Self {
        Self {
            focus: Some(Arc::new(focus)),
            budget: None,
        }
    }

    pub fn focus(&self) -> Option<Arc<JsonataFocus>> {
        self.focus.clone()
    }

    /// Attaches an evaluation budget, consuming and returning `self` for
    /// chaining at call-dispatch sites.
    pub(crate) fn with_budget(mut self, budget: Option<Arc<Budget>>) -> Self {
        self.budget = budget;
        self
    }

    pub(crate) fn budget(&self) -> Option<&Arc<Budget>> {
        self.budget.as_ref()
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

    /// Whether this callable is a *tail-call thunk*: an arity-0 closure produced
    /// by tail-call optimisation whose body is a deferred function call. Such a
    /// value must be driven to completion by the trampoline in
    /// [`JsonFunction::call_forced`] rather than treated as a result. Only the
    /// lambda implementation overrides this.
    fn is_thunk(&self) -> bool {
        false
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync);
}

/// Default backstop for the number of successive tail calls the trampoline
/// drives before giving up with `U1001`. Tail recursion runs in O(1) native
/// stack, so this is not a stack bound — it is only a backstop that terminates a
/// non-productive infinite tail loop (mirroring the reference engine's timebox,
/// which our harness does not enforce). It is set an order of magnitude above
/// the deepest tail recursion in the compatibility suite (~6.5k) so it never
/// caps a real computation, while still stopping a runaway loop promptly. Used
/// only when a call reaches [`JsonFunction::call_forced`] without a per-evaluation
/// [`Budget`] (e.g. a directly-invoked callable); an evaluation started through
/// the public API always carries a `Budget` whose (configurable) limit wins.
pub(crate) const MAX_TAIL_CALL_STEPS: usize = 100_000;

/// Per-evaluation execution budget: bounds runaway recursion and carries the
/// on-demand stack-growth tuning knobs. Exactly one instance is created per
/// top-level evaluation and shared — via `Arc` — by every lexical scope
/// ([`crate::evaluator`] `Bindings`) and every [`FunctionContext`] within that
/// evaluation.
///
/// Owning the depth counter here, rather than in a `thread_local!`, is what
/// makes the non-tail recursion guard correct under `evaluate_async` on a
/// work-stealing multi-thread executor: a single evaluation's `BoxFuture` may be
/// polled — and its guards created and dropped — on *different* threads, so a
/// per-thread counter would drift (leaking depth on one thread, underflowing on
/// another) and eventually raise a spurious `U1001`. An atomic owned by the
/// evaluation increments and decrements the same cell regardless of thread, and
/// concurrent evaluations each get their own `Budget`, so they never interfere.
#[derive(Debug)]
pub(crate) struct Budget {
    /// Current non-tail lambda-call nesting depth (calls live on the native
    /// stack simultaneously). Tail calls do not count — the trampoline unwinds
    /// each before the next.
    non_tail_depth: AtomicUsize,
    /// Maximum non-tail nesting depth; `None` disables the guard.
    max_non_tail_depth: Option<usize>,
    /// Maximum successive tail-call steps; `None` disables the backstop.
    max_tail_call_steps: Option<usize>,
    /// Grow the native stack when fewer than this many bytes remain.
    stack_red_zone: usize,
    /// Size of each fresh stack segment allocated on demand.
    stack_grow_size: usize,
}

impl Budget {
    pub(crate) fn new(
        max_non_tail_depth: Option<usize>,
        max_tail_call_steps: Option<usize>,
        stack_red_zone: usize,
        stack_grow_size: usize,
    ) -> Self {
        Self {
            non_tail_depth: AtomicUsize::new(0),
            max_non_tail_depth,
            max_tail_call_steps,
            stack_red_zone,
            stack_grow_size,
        }
    }

    pub(crate) fn max_tail_call_steps(&self) -> Option<usize> {
        self.max_tail_call_steps
    }

    pub(crate) fn stack_red_zone(&self) -> usize {
        self.stack_red_zone
    }

    pub(crate) fn stack_grow_size(&self) -> usize {
        self.stack_grow_size
    }

    /// Enters one level of non-tail recursion. Returns `Err(U1001)` (having
    /// rolled the increment back) if it would exceed `max_non_tail_depth`.
    pub(crate) fn enter_non_tail(&self) -> Result<(), JsonError> {
        let depth = self.non_tail_depth.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(max) = self.max_non_tail_depth {
            if depth > max {
                self.non_tail_depth.fetch_sub(1, Ordering::Relaxed);
                return Err(JsonError::new(
                    "U1001",
                    "Stack overflow error: non-terminating recursive function call",
                ));
            }
        }
        Ok(())
    }

    /// Leaves one level of non-tail recursion.
    pub(crate) fn leave_non_tail(&self) {
        self.non_tail_depth.fetch_sub(1, Ordering::Relaxed);
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
