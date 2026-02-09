use std::any::Any;
use std::sync::Arc;

use futures::executor::block_on;
use futures::future::BoxFuture;
use jsonata_rust::types::{FunctionContext, JsonCallable, JsonError, JsonFunction, JsonValue};
use jsonata_rust::FunctionRegistry;

#[derive(Clone)]
struct TripleCallable;

impl JsonCallable for TripleCallable {
    fn call(
        &self,
        _ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
        let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
        Box::pin(async move {
            if let JsonValue::Number(value) = input {
                return Ok(JsonValue::Number(value * 3.0));
            }
            Ok(JsonValue::Undefined)
        })
    }

    fn arity(&self) -> Option<usize> {
        Some(1)
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}

fn main() {
    let mut registry = FunctionRegistry::with_builtins();
    registry.insert("triple", JsonFunction::new(Arc::new(TripleCallable)));

    let triple = registry.get("triple").expect("triple should be registered");
    let out = block_on(triple.call(FunctionContext::empty(), vec![JsonValue::Number(7.0)]))
        .expect("triple call should succeed");

    println!("triple(7) = {:?}", out);
}
