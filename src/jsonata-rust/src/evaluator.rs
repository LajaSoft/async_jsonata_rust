use std::collections::HashMap;
use std::future::Future;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{SystemTime, UNIX_EPOCH};

use futures::future::BoxFuture;
use serde_json::Value;

use crate::error::Error;
use crate::functions::core;
use crate::types::{Budget, JsonArray, JsonFunction, JsonValue};

mod callable;
mod expressions;
mod lambda;
mod ops;
mod path;
mod signature;
mod transform;
mod value;

/// A lexical environment frame: the variable map plus the current evaluation's
/// shared [`Budget`]. It derefs to the underlying `HashMap`, so existing
/// `get` / `insert` / `iter` / `contains_key` usage is unchanged; cloning a
/// frame (as every nested scope does) shares the *same* `Budget` `Arc`, so the
/// whole evaluation counts recursion against one owner instead of a thread-local.
#[derive(Clone)]
pub(crate) struct Bindings {
    vars: HashMap<String, JsonValue>,
    budget: Arc<Budget>,
}

impl Bindings {
    fn from_map(vars: HashMap<String, JsonValue>, budget: Arc<Budget>) -> Self {
        Self { vars, budget }
    }

    fn budget(&self) -> &Arc<Budget> {
        &self.budget
    }
}

impl Deref for Bindings {
    type Target = HashMap<String, JsonValue>;

    fn deref(&self) -> &Self::Target {
        &self.vars
    }
}

impl DerefMut for Bindings {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.vars
    }
}

const EVAL_MILLIS_BINDING: &str = "__jsonata_eval_millis";

fn monotonic_eval_millis() -> i64 {
    static LAST_MILLIS: AtomicI64 = AtomicI64::new(0);
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0);
    loop {
        let previous = LAST_MILLIS.load(Ordering::Relaxed);
        let candidate = if now_ms > previous {
            now_ms
        } else {
            previous + 1
        };
        if LAST_MILLIS
            .compare_exchange(previous, candidate, Ordering::SeqCst, Ordering::Relaxed)
            .is_ok()
        {
            return candidate;
        }
    }
}

pub(crate) async fn evaluate_expression_async(
    ast: &Value,
    input: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    budget: Arc<Budget>,
) -> Result<JsonValue, Error> {
    let empty = HashMap::new();
    evaluate_expression_with_bindings_async(ast, input, functions, &empty, budget).await
}

pub(crate) async fn evaluate_expression_with_bindings_async(
    ast: &Value,
    input: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &HashMap<String, JsonValue>,
    budget: Arc<Budget>,
) -> Result<JsonValue, Error> {
    let mut vars = bindings.clone();
    if !vars.contains_key(EVAL_MILLIS_BINDING) {
        vars.insert(
            EVAL_MILLIS_BINDING.to_owned(),
            JsonValue::Number(monotonic_eval_millis() as f64),
        );
    }
    let eval_bindings = Bindings::from_map(vars, budget);

    // Mirror upstream JSONata: if the input document is a plain JSON array (not
    // already a sequence) wrap it in a singleton outer-wrapper sequence so a
    // relative path treats the whole array as a single context item. The root
    // `$`/`$$` variables still resolve to the original (unwrapped) array via the
    // outer-wrapper unwrapping in `eval_variable`.
    // A previous evaluation may have left a tuple-stream ancestry carrier on the
    // thread-local side channel; clear it so it cannot leak into this run.
    path::clear_tuple_carrier();

    let document = match input {
        JsonValue::Array(array) if !array.is_sequence => JsonValue::Array(JsonArray::new(
            vec![input.clone()],
            true,
            true,
        )),
        other => other.clone(),
    };

    eval(ast, &document, &document, functions, &eval_bindings).await
}

/// Wraps a future so every poll runs with a guaranteed minimum of native stack,
/// allocating a fresh segment on demand (via `stacker`) when the remaining stack
/// drops into the `red_zone`. The two sizes come from the evaluation's
/// [`Budget`] (see `EvaluatorOptions`), so callers can tune them.
///
/// Because the whole evaluator is a chain of boxed `eval` futures, polling a
/// deeply nested expression consumes native stack proportional to the recursion
/// depth. Wrapping each `eval` lets those expressions run on a small base stack
/// that grows only as needed — replacing the previous approach of pre-reserving
/// a single enormous (hundreds of MiB) stack, which both wasted address space
/// and still hard-crashed once a recursion outran it. The first `maybe_grow` in
/// a poll cascade establishes the stack limit, so the many nested `eval`s within
/// it merely perform a cheap remaining-stack check and never re-grow.
struct GrowStack<F> {
    inner: F,
    red_zone: usize,
    grow_size: usize,
}

impl<F: Future> Future for GrowStack<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        // SAFETY: `inner` is structurally pinned and never moved out of `self`;
        // `red_zone`/`grow_size` are `Copy` scalars read by value, not pinned.
        let this = unsafe { self.get_unchecked_mut() };
        let (red_zone, grow_size) = (this.red_zone, this.grow_size);
        let inner = unsafe { Pin::new_unchecked(&mut this.inner) };
        stacker::maybe_grow(red_zone, grow_size, || inner.poll(cx))
    }
}

pub(super) fn eval<'a>(
    node: &'a Value,
    input: &'a JsonValue,
    focus: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    let red_zone = bindings.budget().stack_red_zone();
    let grow_size = bindings.budget().stack_grow_size();
    Box::pin(GrowStack { red_zone, grow_size, inner: async move {
    let node_type = node.get("type").and_then(Value::as_str).unwrap_or_default();

    let mut result = match node_type {
        "path" => path::eval_path(node, input, focus, functions, bindings).await,
        "name" => {
            let name = node
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Ok(core::lookup(focus, name))
        }
        "variable" => callable::eval_variable(node, input, focus, functions, bindings),
        "string" => Ok(JsonValue::String(
            node.get("value")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        )),
        "number" => Ok(JsonValue::Number(
            node.get("value").and_then(Value::as_f64).unwrap_or(0.0),
        )),
        "value" => Ok(value::json_value_from_serde(
            node.get("value").unwrap_or(&Value::Null),
        )),
        "regex" => callable::eval_regex(node, focus, bindings).await,
        "function" => callable::eval_function(node, input, focus, functions, bindings).await,
        "binary" => ops::eval_binary(node, input, focus, functions, bindings).await,
        "apply" => callable::eval_apply(node, input, focus, functions, bindings).await,
        "partial" => callable::eval_partial(node, input, focus, functions, bindings).await,
        "block" => expressions::eval_block(node, input, focus, functions, bindings).await,
        "unary" => expressions::eval_unary(node, input, focus, functions, bindings).await,
        "bind" => {
            let (name, mut value) =
                expressions::eval_bind(node, input, focus, functions, bindings).await?;
            // A bind that is the sole body of a lambda (`function(){ $f := ... }`)
            // is evaluated here rather than via `eval_block`; still rebind a
            // bound function to its own name so it (and any nested closures it
            // returns) can refer to itself recursively, matching the block path.
            if let JsonValue::Function(function) = &value {
                if let Some(rebound) = lambda::bind_recursive_name(function, &name) {
                    value = JsonValue::Function(rebound);
                }
            }
            Ok(value)
        }
        "lambda" => lambda::eval_lambda(node, input, focus, functions, bindings),
        "condition" => expressions::eval_condition(node, input, focus, functions, bindings).await,
        "wildcard" => Ok(path::apply_wildcard(focus)),
        "descendant" => Ok(path::apply_descendant(focus)),
        "parent" => path::eval_parent(node, bindings),
        "sort" => path::apply_sort_step(node, input, focus, functions, bindings).await,
        _ => Err(Error::new(
            "E2001",
            format!("Unsupported AST node type: {node_type}"),
        )),
    }?;

    if let Some(predicates) = node.get("predicate").and_then(Value::as_array) {
        for predicate in predicates {
            let expr = predicate.get("expr").unwrap_or(predicate);
            result = path::apply_predicate_expr(expr, input, &result, functions, bindings).await?;
        }
    }

    if node_type != "path" {
        if let Some(group) = node.get("group") {
            result = path::apply_group_expression(group, input, &result, functions, bindings).await?;
        }
    }

    if node_type == "path"
        && node
            .get("keepSingletonArray")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return match result {
            JsonValue::Undefined => Ok(JsonValue::Undefined),
            JsonValue::Array(array) => {
                if array.is_sequence || !array.outer_wrapper {
                    return Ok(JsonValue::Array(array));
                }
                let wrapped = JsonValue::Array(array);
                Ok(JsonValue::Array(JsonArray::new(vec![wrapped], true, false)))
            }
            other => Ok(JsonValue::Array(JsonArray::new(vec![other], true, false))),
        };
    }

    // The `[]` postfix (`keepArray`) forces a singleton sequence to remain an
    // array rather than being unwrapped (mirrors upstream `expr.keepArray`
    // setting `result.keepSingleton`). Materialise the sequence (drop the
    // `is_sequence` flag) so it survives later sequence normalisation.
    let keep_array = node
        .get("keepArray")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if keep_array {
        if let JsonValue::Array(array) = &result {
            if array.is_sequence {
                return Ok(match array.elements.len() {
                    0 => JsonValue::Undefined,
                    _ => JsonValue::Array(JsonArray::new(array.elements.clone(), false, false)),
                });
            }
        }
    }

    Ok(ops::normalize_sequence(result))
    } })
}
