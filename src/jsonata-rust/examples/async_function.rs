use std::any::Any;
use std::sync::Arc;

use futures::executor::block_on;
use futures::future::BoxFuture;
use jsonata_rust::functions::core;
use jsonata_rust::types::{
    FunctionContext, JsonArray, JsonCallable, JsonError, JsonFunction, JsonValue,
};

#[derive(Clone)]
struct DoubleCallable;

impl JsonCallable for DoubleCallable {
    fn call(
        &self,
        _ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
        let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
        Box::pin(async move {
            if let JsonValue::Number(value) = input {
                return Ok(JsonValue::Number(value * 2.0));
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
    let input = JsonValue::Array(JsonArray::new(
        vec![
            JsonValue::Number(1.0),
            JsonValue::Number(2.0),
            JsonValue::Number(3.0),
        ],
        true,
        false,
    ));

    let callable = JsonValue::Function(JsonFunction::new(Arc::new(DoubleCallable)));
    let output = block_on(core::map(FunctionContext::empty(), input, callable))
        .expect("async map should succeed");
    println!("Output: {:?}", output);
}
