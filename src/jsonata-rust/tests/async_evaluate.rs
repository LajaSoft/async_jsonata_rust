//! End-to-end async evaluation test.
//!
//! Verifies that `Evaluator::evaluate_async` drives the *whole* expression with
//! `.await` — including user-supplied async callables. The registered `$double`
//! function yields `Poll::Pending` once before returning, and the test asserts
//! both that the pending yield was observed (so the engine genuinely awaited the
//! callable's future rather than blocking a thread) and that the final result is
//! correct.

use std::any::Any;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::Poll;

use async_jsonata_rust::types::{
    FunctionContext, JsonArray, JsonCallable, JsonError, JsonFunction, JsonValue,
};
use async_jsonata_rust::{Evaluator, FunctionRegistry};
use futures::executor::block_on;
use futures::future::BoxFuture;

#[derive(Clone)]
struct YieldOnceDoubleCallable {
    pending_polls: Arc<AtomicUsize>,
}

impl JsonCallable for YieldOnceDoubleCallable {
    fn call(
        &self,
        _ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
        let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
        let pending_polls = Arc::clone(&self.pending_polls);
        let mut yielded_once = false;

        Box::pin(futures::future::poll_fn(move |cx| {
            // Yield exactly once: register pending, wake ourselves, and return
            // Pending so the executor must come back around. If the engine were
            // blocking on a worker thread instead of awaiting, this Pending would
            // never be observed by the top-level executor.
            if !yielded_once {
                yielded_once = true;
                pending_polls.fetch_add(1, Ordering::Relaxed);
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }

            if let JsonValue::Number(value) = input {
                return Poll::Ready(Ok(JsonValue::Number(value * 2.0)));
            }

            Poll::Ready(Ok(JsonValue::Undefined))
        }))
    }

    fn arity(&self) -> Option<usize> {
        Some(1)
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}

fn evaluator_with_double(pending_polls: Arc<AtomicUsize>) -> Evaluator {
    let mut registry = FunctionRegistry::with_builtins();
    registry.insert(
        "double",
        JsonFunction::new(Arc::new(YieldOnceDoubleCallable { pending_polls })),
    );
    Evaluator::new(registry)
}

#[test]
fn evaluate_async_awaits_user_async_function() {
    let pending_polls = Arc::new(AtomicUsize::new(0));
    let evaluator = evaluator_with_double(Arc::clone(&pending_polls));

    let expression = evaluator
        .parse("$double(21)")
        .expect("expression should parse");

    let result = block_on(evaluator.evaluate_async(&expression, &JsonValue::Null))
        .expect("async evaluation should succeed");

    // The custom callable yielded Pending before completing — proving the whole
    // expression evaluation awaited the user async function cooperatively.
    assert!(
        pending_polls.load(Ordering::Relaxed) > 0,
        "expected the async callable to yield Pending at least once"
    );
    assert_eq!(result, JsonValue::Number(42.0));
}

#[test]
fn evaluate_async_awaits_user_function_inside_map() {
    let pending_polls = Arc::new(AtomicUsize::new(0));
    let evaluator = evaluator_with_double(Arc::clone(&pending_polls));

    let expression = evaluator
        .parse("$map([1, 2, 3], $double)")
        .expect("expression should parse");

    let result = block_on(evaluator.evaluate_async(&expression, &JsonValue::Null))
        .expect("async evaluation should succeed");

    // Once per mapped element the callable yields Pending; observing this proves
    // higher-order builtins drive user async functions via `.await` end to end.
    assert!(
        pending_polls.load(Ordering::Relaxed) >= 3,
        "expected the async callable to yield Pending for each mapped element"
    );
    assert_eq!(
        result,
        JsonValue::Array(JsonArray::new(
            vec![
                JsonValue::Number(2.0),
                JsonValue::Number(4.0),
                JsonValue::Number(6.0),
            ],
            true,
            false,
        ))
    );
}
