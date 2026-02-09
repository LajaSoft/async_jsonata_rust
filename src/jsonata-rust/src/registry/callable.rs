use futures::future::BoxFuture;

use crate::types::{FunctionContext, JsonCallable, JsonError, JsonValue};

#[derive(Clone)]
pub(super) struct BuiltinCallable {
    arity: Option<usize>,
    handler: fn(&[JsonValue]) -> Result<JsonValue, JsonError>,
    async_handler:
        Option<fn(FunctionContext, &[JsonValue]) -> BoxFuture<'static, Result<JsonValue, JsonError>>>,
}

impl BuiltinCallable {
    pub(super) fn sync_fn(
        arity: Option<usize>,
        handler: fn(&[JsonValue]) -> Result<JsonValue, JsonError>,
    ) -> Self {
        Self {
            arity,
            handler,
            async_handler: None,
        }
    }

    pub(super) fn async_fn(
        arity: Option<usize>,
        handler: fn(FunctionContext, &[JsonValue]) -> BoxFuture<'static, Result<JsonValue, JsonError>>,
    ) -> Self {
        Self {
            arity,
            handler: |_| Ok(JsonValue::Undefined),
            async_handler: Some(handler),
        }
    }
}

impl JsonCallable for BuiltinCallable {
    fn call(
        &self,
        ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
        if let Some(async_handler) = self.async_handler {
            return async_handler(ctx, &args);
        }

        let handler = self.handler;
        let result = handler(&args);
        Box::pin(async move { result })
    }

    fn arity(&self) -> Option<usize> {
        self.arity
    }

    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}
