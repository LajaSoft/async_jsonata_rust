use napi::bindgen_prelude::*;
use napi::{CallContext, Env, JsObject, JsUnknown, ValueType};
use napi_derive::napi;

use jsonata_rust::functions::math as math_impl;

fn option_number_to_js(env: &Env, value: Option<f64>) -> napi::Result<JsUnknown> {
    match value {
        Some(num) => env.create_double(num).map(|n| n.into_unknown()),
        None => env.get_undefined().map(|u| u.into_unknown()),
    }
}

fn extract_numeric_args(ctx: &CallContext) -> napi::Result<Option<Vec<f64>>> {
    if ctx.length == 0 {
        return Ok(None);
    }
    let first: JsUnknown = ctx.get(0)?;
    match first.get_type()? {
        ValueType::Undefined | ValueType::Null => Ok(None),
        ValueType::Number => {
            let value: f64 = ctx.get(0)?;
            Ok(Some(vec![value]))
        }
        ValueType::Object => {
            let object: JsObject = ctx.get(0)?;
            if object.is_array()? {
                let values: Vec<f64> = ctx.get(0)?;
                Ok(Some(values))
            } else {
                Err(Error::new(
                    Status::InvalidArg,
                    "Expected an array of numbers for math helper",
                ))
            }
        }
        _ => Err(Error::new(
            Status::InvalidArg,
            "Unsupported argument type for math helper",
        )),
    }
}

fn register_math(env: &Env, exports: &mut JsObject) -> napi::Result<()> {
    let sum = env.create_function_from_closure("sum", |ctx| {
        let values = extract_numeric_args(&ctx)?;
        option_number_to_js(ctx.env, math_impl::sum(values.as_ref().map(|v| v.as_slice())))
    })?;
    exports.set_named_property("sum", sum)?;

    let max = env.create_function_from_closure("max", |ctx| {
        let values = extract_numeric_args(&ctx)?;
        option_number_to_js(ctx.env, math_impl::max(values.as_ref().map(|v| v.as_slice())))
    })?;
    exports.set_named_property("max", max)?;

    let min = env.create_function_from_closure("min", |ctx| {
        let values = extract_numeric_args(&ctx)?;
        option_number_to_js(ctx.env, math_impl::min(values.as_ref().map(|v| v.as_slice())))
    })?;
    exports.set_named_property("min", min)?;

    let average = env.create_function_from_closure("average", |ctx| {
        let values = extract_numeric_args(&ctx)?;
        option_number_to_js(
            ctx.env,
            math_impl::average(values.as_ref().map(|v| v.as_slice())),
        )
    })?;
    exports.set_named_property("average", average)?;

    Ok(())
}

fn register_unimplemented(env: &Env, exports: &mut JsObject) -> napi::Result<()> {
    const UNIMPLEMENTED: &[&str] = &[
        "count",
        "string",
        "substring",
        "substringBefore",
        "substringAfter",
        "lowercase",
        "uppercase",
        "length",
        "trim",
        "pad",
        "match",
        "contains",
        "replace",
        "split",
        "join",
        "formatNumber",
        "formatBase",
        "number",
        "floor",
        "ceil",
        "round",
        "abs",
        "sqrt",
        "power",
        "random",
        "boolean",
        "not",
        "map",
        "zip",
        "filter",
        "single",
        "foldLeft",
        "sift",
        "keys",
        "lookup",
        "append",
        "exists",
        "spread",
        "merge",
        "reverse",
        "each",
        "error",
        "assert",
        "type",
        "sort",
        "shuffle",
        "distinct",
        "base64encode",
        "base64decode",
        "encodeUrlComponent",
        "encodeUrl",
        "decodeUrlComponent",
        "decodeUrl",
    ];

    for name in UNIMPLEMENTED {
        let message = format!("Function '{}' is not yet implemented in Rust", name);
        let func = env.create_function_from_closure(name, move |_| -> napi::Result<JsUnknown> {
            Err(Error::new(Status::GenericFailure, message.clone()))
        })?;
        exports.set_named_property(*name, func)?;
    }

    Ok(())
}

#[napi(js_name = "load_functions")]
pub fn load_functions(env: Env) -> napi::Result<JsObject> {
    let mut exports = env.create_object()?;
    register_math(&env, &mut exports)?;
    register_unimplemented(&env, &mut exports)?;
    Ok(exports)
}
