use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::future::BoxFuture;
use serde_json::Value;

use crate::error::Error;
use crate::types::{
    FunctionContext, JsonCallable, JsonError, JsonFunction, JsonValue, JsonataFocus,
};

use super::{eval, Bindings};

#[derive(Clone)]
struct LambdaCallable {
    arg_names: Vec<String>,
    body: Value,
    body_is_thunk: bool,
    is_thunk: bool,
    recursive_name: Option<String>,
    signature: Option<super::signature::Signature>,
    captured_input: JsonValue,
    captured_focus: JsonValue,
    captured_bindings: Bindings,
    functions: HashMap<String, JsonFunction>,
    /// Shared, mutable environment frame for let-rec / mutual recursion. When a
    /// group of `:=` bindings in the same block defines sibling functions, every
    /// lambda in that group captures a clone of the same `Arc`; after all binds
    /// are evaluated, the block populates this frame with the final sibling
    /// values. At call time these overlay `captured_bindings`, so a function can
    /// see siblings that were bound later in the same block (knot-tying).
    shared_frame: Option<Arc<RwLock<Bindings>>>,
}

const MAX_LAMBDA_CALL_DEPTH: usize = 7_000;
static LAMBDA_CALL_DEPTH: AtomicUsize = AtomicUsize::new(0);

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

struct LambdaDepthGuard;

impl LambdaDepthGuard {
    fn enter() -> std::result::Result<Self, JsonError> {
        let depth = LAMBDA_CALL_DEPTH.fetch_add(1, Ordering::SeqCst) + 1;
        if depth > MAX_LAMBDA_CALL_DEPTH {
            LAMBDA_CALL_DEPTH.fetch_sub(1, Ordering::SeqCst);
            return Err(JsonError::new(
                "U1001",
                "Stack overflow error: non-terminating recursive function call",
            ));
        }
        Ok(Self)
    }
}

impl Drop for LambdaDepthGuard {
    fn drop(&mut self) {
        LAMBDA_CALL_DEPTH.fetch_sub(1, Ordering::SeqCst);
    }
}

impl JsonCallable for LambdaCallable {
    fn call(
        &self,
        ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
        let depth_guard = match LambdaDepthGuard::enter() {
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
        let body_is_thunk = self.body_is_thunk;
        let captured_input = self.captured_input.clone();
        let functions = self.functions.clone();

        Box::pin(async move {
            let _depth_guard = depth_guard;
            let result: Result<JsonValue, Error> = async {
                let mut value =
                    eval(&body, &captured_input, &focus, &functions, &call_bindings).await?;

                if body_is_thunk {
                    // The body evaluated to a tail-call thunk (an arity-0 thunk
                    // lambda). Force thunks repeatedly until a non-thunk value is
                    // produced. Crucially we only continue while the result is
                    // itself a thunk: a thunk-bodied lambda may legitimately
                    // return a *real* function value (e.g. a Y/Z combinator
                    // returns the recursive function), which must be returned
                    // as-is rather than being force-called with no arguments.
                    loop {
                        let callable = match &value {
                            JsonValue::Function(callable) if is_thunk_function(callable) => {
                                callable.clone()
                            }
                            _ => break,
                        };
                        let ctx = FunctionContext::with_focus(JsonataFocus::new(focus.clone()));
                        value = callable.call(ctx, Vec::new()).await.map_err(Error::from)?;
                    }
                }

                Ok(value)
            }
            .await;

            result.map_err(error_to_json_error)
        })
    }

    fn arity(&self) -> Option<usize> {
        Some(self.arg_names.len())
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
    let body_is_thunk = node
        .get("thunk")
        .and_then(Value::as_bool)
        .or_else(|| body.get("thunk").and_then(Value::as_bool))
        .unwrap_or(false);

    let signature = node
        .get("signature")
        .and_then(Value::as_str)
        .and_then(super::signature::Signature::parse);

    let callable = LambdaCallable {
        arg_names,
        body,
        body_is_thunk,
        is_thunk: node.get("thunk").and_then(Value::as_bool).unwrap_or(false),
        recursive_name: None,
        signature,
        captured_input: input.clone(),
        captured_focus: focus.clone(),
        captured_bindings: bindings.clone(),
        functions: functions.clone(),
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
    frame: &Arc<RwLock<Bindings>>,
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
pub(super) fn new_shared_frame() -> Arc<RwLock<Bindings>> {
    Arc::new(RwLock::new(Bindings::new()))
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

pub(super) fn is_thunk_function(function: &JsonFunction) -> bool {
    function
        .as_callable()
        .as_any()
        .downcast_ref::<LambdaCallable>()
        .map(|lambda| lambda.is_thunk)
        .unwrap_or(false)
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
