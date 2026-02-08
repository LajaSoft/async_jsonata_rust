use napi::bindgen_prelude::*;
use napi::sys;
use napi::threadsafe_function::{
    ThreadsafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode,
};
use napi::{Env, Status, ValueType};
use napi_derive::napi;
use regex::RegexBuilder;

use futures::channel::oneshot;
use futures::future::BoxFuture;

use jsonata_rust::functions::{core as core_impl, math as math_impl, strings as strings_impl};
use jsonata_rust::registry;
use jsonata_rust::types::{
    CallbackHandle, FunctionContext, JsonArray, JsonCallable, JsonError, JsonFunction,
    JsonObject, JsonValue, JsonataFocus,
};

use std::any::Any;
use std::mem;
use std::sync::{Arc, Mutex};

mod conversion;
mod function_registry;

#[path = "lib/types.rs"]
mod bridge_types;
#[path = "lib/handles.rs"]
mod bridge_handles;
#[path = "lib/convert.rs"]
mod bridge_convert;
#[path = "lib/register_math.rs"]
mod bridge_register_math;
#[path = "lib/register_core.rs"]
mod bridge_register_core;
#[path = "lib/regex_utils.rs"]
mod bridge_regex_utils;
#[path = "lib/regex_match.rs"]
mod bridge_regex_match;
#[path = "lib/register_strings.rs"]
mod bridge_register_strings;
#[path = "lib/callable.rs"]
mod bridge_callable;
#[path = "lib/callable_registry.rs"]
mod bridge_callable_registry;

use bridge_types::*;
use bridge_handles::*;
use bridge_callable::*;
use bridge_callable_registry::*;
use bridge_convert::*;
use bridge_register_core::*;
use bridge_register_math::*;
use bridge_regex_utils::*;
use bridge_regex_match::*;
use bridge_register_strings::*;

fn register_unimplemented(env: &Env, exports: &mut JsObject) -> napi::Result<()> {
    const UNIMPLEMENTED: &[&str] = &[];

    for name in UNIMPLEMENTED {
        let message = format!("Function '{}' is not yet implemented in Rust", name);
        let func = env.create_function_from_closure::<(), sys::napi_value, _>(name, move |_| {
            Err(Error::new(Status::GenericFailure, message.clone()))
        })?;
        exports.set_named_property(*name, func)?;
    }

    Ok(())
}

#[napi(js_name = "load_functions")]
pub fn load_functions(env: Env) -> napi::Result<JsUnknown<'static>> {
    let mut exports = Object::new(&env)?;

    function_registry::register_all_functions(&env, &mut exports)?;
    register_math(&env, &mut exports)?;
    register_core(&env, &mut exports)?;
    register_strings(&env, &mut exports)?;
    register_unimplemented(&env, &mut exports)?;

    let unknown = exports.to_unknown();
    Ok(unsafe { mem::transmute::<JsUnknown<'_>, JsUnknown<'static>>(unknown) })
}

#[napi(js_name = "parseExpression")]
pub fn parse_expression(
    env: Env,
    source: String,
    recover: Option<bool>,
) -> napi::Result<JsUnknown<'static>> {
    let recover = recover.unwrap_or(false);
    let mut result_object = Object::new(&env)?;

    match jsonata_rust::parser::parse_expression(&source, recover) {
        Ok(ast) => {
            result_object.set_named_property("ok", true)?;
            let ast_js = crate::conversion::serde_value_to_js(&env, &ast)?;
            result_object.set_named_property("ast", ast_js)?;
        }
        Err(err) => {
            result_object.set_named_property("ok", false)?;
            let js_error = crate::conversion::parser_error_to_js(&env, &err)?;
            result_object.set_named_property("error", js_error)?;
        }
    }

    let unknown = result_object.to_unknown();
    Ok(unsafe { mem::transmute::<JsUnknown<'_>, JsUnknown<'static>>(unknown) })
}
