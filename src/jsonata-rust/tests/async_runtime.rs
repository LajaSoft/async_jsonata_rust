use std::any::Any;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::Poll;

use futures::executor::block_on;
use futures::future::BoxFuture;
use jsonata_rust::functions::core;
use jsonata_rust::types::{
    FunctionContext, JsonArray, JsonCallable, JsonError, JsonFunction, JsonValue,
};

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

#[test]
fn async_map_transitions_pending_to_ready() {
    let pending_polls = Arc::new(AtomicUsize::new(0));
    let callable = JsonValue::Function(JsonFunction::new(Arc::new(YieldOnceDoubleCallable {
        pending_polls: Arc::clone(&pending_polls),
    })));

    let input = JsonValue::Array(JsonArray::new(
        vec![
            JsonValue::Number(1.0),
            JsonValue::Number(2.0),
            JsonValue::Number(3.0),
        ],
        true,
        false,
    ));

    let out = block_on(core::map(FunctionContext::empty(), input, callable))
        .expect("async map should succeed");

    assert!(pending_polls.load(Ordering::Relaxed) > 0);
    assert_eq!(
        out,
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
