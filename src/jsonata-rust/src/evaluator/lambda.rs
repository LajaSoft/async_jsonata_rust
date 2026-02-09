use std::collections::HashMap;
use std::sync::Arc;

use futures::executor::block_on;
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
    captured_input: JsonValue,
    captured_focus: JsonValue,
    captured_bindings: Bindings,
    functions: HashMap<String, JsonFunction>,
}

impl JsonCallable for LambdaCallable {
    fn call(
        &self,
        ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
        let mut call_bindings = self.captured_bindings.clone();
        for (index, name) in self.arg_names.iter().enumerate() {
            let value = args.get(index).cloned().unwrap_or(JsonValue::Undefined);
            call_bindings.insert(name.clone(), value.clone());
            call_bindings.insert(format!("${name}"), value);
        }

        let focus = match ctx.focus() {
            Some(focus) => focus.input.clone(),
            None => self.captured_focus.clone(),
        };
        let body = self.body.clone();
        let body_is_thunk = self.body_is_thunk;
        let captured_input = self.captured_input.clone();
        let functions = self.functions.clone();

        Box::pin(async move {
            let handle = std::thread::spawn(move || {
                let mut value = eval(&body, &captured_input, &focus, &functions, &call_bindings)?;

                if body_is_thunk {
                    let mut depth = 0usize;
                    while let JsonValue::Function(callable) = value {
                        if depth >= 32 {
                            return Err(Error::new("U1001", "Tail-call trampoline depth exceeded"));
                        }
                        let ctx = FunctionContext::with_focus(JsonataFocus::new(focus.clone()));
                        value = block_on(callable.call(ctx, Vec::new())).map_err(Error::from)?;
                        depth += 1;
                    }
                }

                Ok(value)
            });

            let result = handle.join().map_err(|_| {
                JsonError::new("D3120", "Lambda execution failed: worker thread panicked")
            })?;

            result.map_err(|err| {
                JsonError::new(
                    "D3120",
                    format!("Lambda execution failed: {}: {}", err.code(), err.message()),
                )
            })
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
    let body_is_thunk = body.get("thunk").and_then(Value::as_bool).unwrap_or(false);

    let callable = LambdaCallable {
        arg_names,
        body,
        body_is_thunk,
        captured_input: input.clone(),
        captured_focus: focus.clone(),
        captured_bindings: bindings.clone(),
        functions: functions.clone(),
    };

    Ok(JsonValue::Function(JsonFunction::new(Arc::new(callable))))
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
