use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;

use futures::future::BoxFuture;
use serde_json::Value;

use crate::error::Error;
use crate::types::{Budget, FunctionContext, JsonCallable, JsonError, JsonFunction, JsonValue};

use super::{eval, Bindings};

#[derive(Clone)]
struct LambdaCallable {
    arg_names: Vec<String>,
    body: Value,
    is_thunk: bool,
    recursive_name: Option<String>,
    signature: Option<super::signature::Signature>,
    captured_input: JsonValue,
    captured_focus: JsonValue,
    captured_bindings: Bindings,
    /// The built-in function registry. It is immutable for the whole evaluation,
    /// so it is shared via `Arc` rather than deep-cloned (≈60 `String` keys) on
    /// every call — which matters for hot lambda loops and deep tail recursion.
    functions: Arc<HashMap<String, JsonFunction>>,
    /// Shared, mutable environment frame for let-rec / mutual recursion. When a
    /// group of `:=` bindings in the same block defines sibling functions, every
    /// lambda in that group captures a clone of the same `Arc`; after all binds
    /// are evaluated, the block populates this frame with the final sibling
    /// values. At call time these overlay `captured_bindings`, so a function can
    /// see siblings that were bound later in the same block (knot-tying). It is a
    /// bare value map, not a `Bindings`: it only carries sibling values that are
    /// merged into a real (budget-bearing) frame at call time.
    shared_frame: Option<Arc<RwLock<HashMap<String, JsonValue>>>>,
}

/// Converts an internal `Error` (with a `String` code) into a `JsonError`
/// (which carries a `&'static str` code) without per-call leaking. JSONata
/// error codes are drawn from a small finite set; the interner leaks each
/// distinct code string at most once, so total allocation stays bounded.
fn error_to_json_error(err: Error) -> JsonError {
    JsonError::new(intern_code(err.code()), err.message().to_owned())
}

fn intern_code(code: &str) -> &'static str {
    use std::collections::HashSet;
    use std::sync::Mutex;
    use std::sync::OnceLock;

    static INTERN: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let set = INTERN.get_or_init(|| Mutex::new(HashSet::new()));
    let mut guard = set.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(existing) = guard.get(code) {
        return existing;
    }
    let leaked: &'static str = Box::leak(code.to_owned().into_boxed_str());
    guard.insert(leaked);
    leaked
}

/// Guards *non-tail* recursion by counting the lambda calls nested on the native
/// call stack at once. Tail calls do not count against it (the trampoline in
/// [`crate::types::JsonFunction::call_forced`] unwinds each one before making the
/// next), so it only bounds genuine stack-consuming recursion. Exceeding the
/// evaluation's configured `max_non_tail_depth` yields `U1001` instead of
/// exhausting memory on a non-terminating recursive function. The native stack
/// itself grows on demand (see `GrowStack` in `evaluator.rs`), so this — not a
/// fixed stack size — is what bounds runaway non-tail recursion.
///
/// The counter lives in the per-evaluation [`Budget`] (owned by the evaluation),
/// **not** a `thread_local!`: under `evaluate_async` on a multi-thread executor
/// the guard may be created and dropped on different threads as the future
/// migrates, which would corrupt a per-thread counter. The `RAII` guard captures
/// the same `Arc<Budget>` it entered, so `enter`/`drop` always adjust one cell.
struct LambdaDepthGuard {
    budget: Arc<Budget>,
}

impl LambdaDepthGuard {
    fn enter(budget: &Arc<Budget>) -> std::result::Result<Self, JsonError> {
        budget.enter_non_tail()?;
        Ok(Self {
            budget: Arc::clone(budget),
        })
    }
}

impl Drop for LambdaDepthGuard {
    fn drop(&mut self) {
        self.budget.leave_non_tail();
    }
}

impl JsonCallable for LambdaCallable {
    fn call(
        &self,
        ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
        let depth_guard = match LambdaDepthGuard::enter(self.captured_bindings.budget()) {
            Ok(guard) => guard,
            Err(err) => return Box::pin(async move { Err(err) }),
        };

        // Validate (and fix up) arguments against the declared signature. The
        // context value is taken from the current focus, matching the reference
        // engine's `validateArguments(signature, args, input)`.
        let args = if let Some(signature) = &self.signature {
            let context = ctx
                .focus()
                .map(|focus| focus.input.clone())
                .unwrap_or_else(|| self.captured_focus.clone());
            match signature.validate(args, &context) {
                Ok(validated) => validated,
                Err(err) => return Box::pin(async move { Err(err) }),
            }
        } else {
            args
        };

        let mut call_bindings = self.captured_bindings.clone();
        // Overlay the shared let-rec frame so sibling functions defined later in
        // the same block become visible (mutual recursion / forward reference).
        if let Some(frame) = &self.shared_frame {
            if let Ok(frame) = frame.read() {
                for (key, value) in frame.iter() {
                    call_bindings.insert(key.clone(), value.clone());
                }
            }
        }
        if let Some(name) = &self.recursive_name {
            let self_value = JsonValue::Function(JsonFunction::new(Arc::new(self.clone())));
            call_bindings.insert(name.clone(), self_value.clone());
            call_bindings.insert(format!("${name}"), self_value);
        }
        for (index, name) in self.arg_names.iter().enumerate() {
            let value = args.get(index).cloned().unwrap_or(JsonValue::Undefined);
            call_bindings.insert(name.clone(), value.clone());
            call_bindings.insert(format!("${name}"), value);
        }

        let _ = ctx;
        let focus = self.captured_focus.clone();
        let body = self.body.clone();
        let is_tail_thunk = self.is_thunk && self.arg_names.is_empty();
        let captured_input = self.captured_input.clone();
        let functions = self.functions.clone();

        Box::pin(async move {
            let _depth_guard = depth_guard;
            let result =
                if is_tail_thunk && body.get("type").and_then(Value::as_str) == Some("function") {
                    super::callable::eval_tail_call(
                        &body,
                        &captured_input,
                        &focus,
                        &functions,
                        &call_bindings,
                    )
                    .await
                } else {
                    eval(&body, &captured_input, &focus, &functions, &call_bindings).await
                };
            // Return the body value as-is — including a tail-call thunk. The
            // single trampoline in `JsonFunction::call_forced` drives thunks to a
            // final value iteratively (O(1) native stack). Driving them here (by
            // re-calling into this method) would instead recurse once per tail
            // step and blow the stack on deep or infinite tail recursion.
            let value = result.map_err(error_to_json_error)?;
            Ok(value)
        })
    }

    fn arity(&self) -> Option<usize> {
        Some(self.arg_names.len())
    }

    fn is_thunk(&self) -> bool {
        self.is_thunk
    }

    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}

pub(super) fn eval_lambda(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let arguments = node
        .get("arguments")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut arg_names = Vec::with_capacity(arguments.len());
    for arg in arguments {
        if let Some(name) = extract_lambda_arg_name(&arg) {
            arg_names.push(name);
        }
    }

    let body = node
        .get("body")
        .cloned()
        .ok_or_else(|| Error::new("E2026", "Lambda missing body"))?;
    let signature = node
        .get("signature")
        .and_then(Value::as_str)
        .and_then(super::signature::Signature::parse);

    let callable = LambdaCallable {
        arg_names,
        body,
        is_thunk: node.get("thunk").and_then(Value::as_bool).unwrap_or(false),
        recursive_name: None,
        signature,
        captured_input: input.clone(),
        captured_focus: focus.clone(),
        captured_bindings: bindings.clone(),
        functions: Arc::new(functions.clone()),
        shared_frame: None,
    };

    Ok(JsonValue::Function(JsonFunction::new(Arc::new(callable))))
}

pub(super) fn bind_recursive_name(function: &JsonFunction, name: &str) -> Option<JsonFunction> {
    let lambda = function
        .as_callable()
        .as_any()
        .downcast_ref::<LambdaCallable>()?;

    let mut rebound = lambda.clone();
    rebound.recursive_name = Some(name.to_owned());

    Some(JsonFunction::new(Arc::new(rebound)))
}

/// Re-binds a lambda so it captures the given shared let-rec frame. Returns
/// `None` for non-lambda functions (built-ins, partials) which cannot
/// participate in knot-tying and are left untouched.
pub(super) fn attach_shared_frame(
    function: &JsonFunction,
    frame: &Arc<RwLock<HashMap<String, JsonValue>>>,
) -> Option<JsonFunction> {
    let lambda = function
        .as_callable()
        .as_any()
        .downcast_ref::<LambdaCallable>()?;

    let mut rebound = lambda.clone();
    rebound.shared_frame = Some(Arc::clone(frame));

    Some(JsonFunction::new(Arc::new(rebound)))
}

/// Creates a fresh, empty shared let-rec frame.
pub(super) fn new_shared_frame() -> Arc<RwLock<HashMap<String, JsonValue>>> {
    Arc::new(RwLock::new(HashMap::new()))
}

/// Returns `true` when the function is a lambda (and therefore can capture a
/// shared frame).
pub(super) fn is_lambda_function(function: &JsonFunction) -> bool {
    function
        .as_callable()
        .as_any()
        .downcast_ref::<LambdaCallable>()
        .is_some()
}

fn extract_lambda_arg_name(arg: &Value) -> Option<String> {
    if let Some(raw) = arg.as_str() {
        let name = raw.trim_start_matches('$').to_owned();
        if !name.is_empty() {
            return Some(name);
        }
    }

    if let Some(raw) = arg.get("value").and_then(Value::as_str) {
        let name = raw.trim_start_matches('$').to_owned();
        if !name.is_empty() {
            return Some(name);
        }
    }

    if arg.get("type").and_then(Value::as_str) == Some("path") {
        let first_step = arg
            .get("steps")
            .and_then(Value::as_array)
            .and_then(|steps| steps.first())?;
        return extract_lambda_arg_name(first_step);
    }

    None
}
