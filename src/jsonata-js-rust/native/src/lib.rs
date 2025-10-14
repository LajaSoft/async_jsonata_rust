use napi::bindgen_prelude::*;
use napi::{CallContext, Env, JsObject, JsUnknown, ValueType};
use napi_derive::napi;

use jsonata_rust::functions::{core as core_impl, math as math_impl, strings as strings_impl};
use jsonata_rust::types::{JsonArray, JsonError, JsonObject, JsonValue};

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
        option_number_to_js(
            ctx.env,
            math_impl::sum(values.as_ref().map(|v| v.as_slice())),
        )
    })?;
    exports.set_named_property("sum", sum)?;

    let max = env.create_function_from_closure("max", |ctx| {
        let values = extract_numeric_args(&ctx)?;
        option_number_to_js(
            ctx.env,
            math_impl::max(values.as_ref().map(|v| v.as_slice())),
        )
    })?;
    exports.set_named_property("max", max)?;

    let min = env.create_function_from_closure("min", |ctx| {
        let values = extract_numeric_args(&ctx)?;
        option_number_to_js(
            ctx.env,
            math_impl::min(values.as_ref().map(|v| v.as_slice())),
        )
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

fn js_unknown_to_json_value(env: &Env, value: JsUnknown) -> napi::Result<JsonValue> {
    match value.get_type()? {
        ValueType::Undefined => Ok(JsonValue::Undefined),
        ValueType::Null => Ok(JsonValue::Null),
        ValueType::Boolean => Ok(JsonValue::Bool(value.coerce_to_bool()?.get_value()?)),
        ValueType::Number => Ok(JsonValue::Number(value.coerce_to_number()?.get_double()?)),
        ValueType::String => Ok(JsonValue::String(
            value.coerce_to_string()?.into_utf8()?.as_str()?.to_owned(),
        )),
        ValueType::BigInt => Ok(JsonValue::String(
            value.coerce_to_string()?.into_utf8()?.as_str()?.to_owned(),
        )),
        ValueType::Object => {
            let object = value.coerce_to_object()?;
            if object.is_array()? {
                let length = object.get_array_length()?;
                let mut elements = Vec::with_capacity(length as usize);
                for index in 0..length {
                    let element: JsUnknown = object.get_element(index)?;
                    elements.push(js_unknown_to_json_value(env, element)?);
                }

                let mut is_sequence = false;
                if object.has_named_property("sequence")? {
                    let flag: JsUnknown = object.get_named_property("sequence")?;
                    is_sequence = match flag.get_type()? {
                        ValueType::Boolean => flag.coerce_to_bool()?.get_value()?,
                        ValueType::Number => flag.coerce_to_number()?.get_double()? != 0.0,
                        ValueType::String => flag
                            .coerce_to_string()?
                            .into_utf8()?
                            .as_str()?
                            .eq_ignore_ascii_case("true"),
                        _ => true,
                    };
                }

                let mut outer_wrapper = false;
                if object.has_named_property("outerWrapper")? {
                    let flag: JsUnknown = object.get_named_property("outerWrapper")?;
                    outer_wrapper = match flag.get_type()? {
                        ValueType::Boolean => flag.coerce_to_bool()?.get_value()?,
                        ValueType::Number => flag.coerce_to_number()?.get_double()? != 0.0,
                        ValueType::String => flag
                            .coerce_to_string()?
                            .into_utf8()?
                            .as_str()?
                            .eq_ignore_ascii_case("true"),
                        _ => true,
                    };
                }

                Ok(JsonValue::Array(JsonArray::new(
                    elements,
                    is_sequence,
                    outer_wrapper,
                )))
            } else {
                let property_names = object.get_property_names()?;
                let total = property_names.get_array_length()?;
                let mut props = Vec::with_capacity(total as usize);
                for index in 0..total {
                    let name_value: JsUnknown = property_names.get_element(index)?;
                    let name = name_value
                        .coerce_to_string()?
                        .into_utf8()?
                        .as_str()?
                        .to_owned();
                    let property: JsUnknown = object.get_named_property(&name)?;
                    let value = js_unknown_to_json_value(env, property)?;
                    props.push((name, value));
                }
                Ok(JsonValue::Object(JsonObject(props)))
            }
        }
        _ => Err(Error::new(
            Status::InvalidArg,
            "Unsupported argument type for Rust implementation",
        )),
    }
}

fn json_value_to_js(env: &Env, value: JsonValue) -> napi::Result<JsUnknown> {
    match value {
        JsonValue::Undefined => env.get_undefined().map(|v| v.into_unknown()),
        JsonValue::Null => env.get_null().map(|v| v.into_unknown()),
        JsonValue::Bool(flag) => env.get_boolean(flag).map(|v| v.into_unknown()),
        JsonValue::Number(num) => env.create_double(num).map(|v| v.into_unknown()),
        JsonValue::String(text) => env.create_string(&text).map(|v| v.into_unknown()),
        JsonValue::Array(array) => {
            let mut js_array: JsObject = env.create_array_with_length(array.elements.len())?;
            for (index, element) in array.elements.into_iter().enumerate() {
                let js_value = json_value_to_js(env, element)?;
                js_array.set_element(index as u32, js_value)?;
            }
            if array.is_sequence {
                let flag = env.get_boolean(true)?;
                js_array.set_named_property("sequence", flag)?;
            }
            if array.outer_wrapper {
                let flag = env.get_boolean(true)?;
                js_array.set_named_property("outerWrapper", flag)?;
            }
            Ok(js_array.into_unknown())
        }
        JsonValue::Object(JsonObject(entries)) => {
            let mut js_object = env.create_object()?;
            for (key, entry_value) in entries {
                let js_value = json_value_to_js(env, entry_value)?;
                js_object.set_named_property(&key, js_value)?;
            }
            Ok(js_object.into_unknown())
        }
    }
}

fn arg_to_json_value(ctx: &CallContext, index: usize) -> napi::Result<JsonValue> {
    if index >= ctx.length {
        return Ok(JsonValue::Undefined);
    }
    let value: JsUnknown = ctx.get(index)?;
    js_unknown_to_json_value(ctx.env, value)
}

fn json_error_to_napi(err: JsonError) -> napi::Error {
    napi::Error::new(
        Status::GenericFailure,
        format!("{}: {}", err.code, err.message),
    )
}

fn register_core(env: &Env, exports: &mut JsObject) -> napi::Result<()> {
    let lookup = env.create_function_from_closure("lookup", |ctx| {
        if ctx.length < 2 {
            return ctx.env.get_undefined().map(|v| v.into_unknown());
        }
        let input = arg_to_json_value(&ctx, 0)?;
        let key: String = ctx.get(1)?;
        let result = core_impl::lookup(&input, &key);
        json_value_to_js(ctx.env, result)
    })?;
    exports.set_named_property("lookup", lookup)?;

    let append = env.create_function_from_closure("append", |ctx| {
        let left = arg_to_json_value(&ctx, 0)?;
        let right = arg_to_json_value(&ctx, 1)?;
        let result = core_impl::append(&left, &right);
        json_value_to_js(ctx.env, result)
    })?;
    exports.set_named_property("append", append)?;

    let exists = env.create_function_from_closure("exists", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        let result = core_impl::exists(&value);
        json_value_to_js(ctx.env, result)
    })?;
    exports.set_named_property("exists", exists)?;

    let keys = env.create_function_from_closure("keys", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        let result = core_impl::keys(&value);
        json_value_to_js(ctx.env, result)
    })?;
    exports.set_named_property("keys", keys)?;

    Ok(())
}

fn register_strings(env: &Env, exports: &mut JsObject) -> napi::Result<()> {
    let string_fn = env.create_function_from_closure("string", |ctx| {
        if ctx.length == 0 {
            return ctx.env.get_undefined().map(|v| v.into_unknown());
        }

        let first: JsUnknown = ctx.get(0)?;
        match first.get_type()? {
            ValueType::Undefined => return ctx.env.get_undefined().map(|v| v.into_unknown()),
            ValueType::Function => return ctx.env.create_string("").map(|v| v.into_unknown()),
            _ => {}
        }

        let prettify = if ctx.length > 1 {
            let flag: JsUnknown = ctx.get(1)?;
            flag.coerce_to_bool()?.get_value()?
        } else {
            false
        };

        let value = js_unknown_to_json_value(ctx.env, first)?;
        match strings_impl::string(&value, prettify) {
            Ok(result) => json_value_to_js(ctx.env, result),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;

    exports.set_named_property("string", string_fn)?;

    let substring_fn = env.create_function_from_closure("substring", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        let start = arg_to_json_value(&ctx, 1)?;
        let length = arg_to_json_value(&ctx, 2)?;
        match strings_impl::substring(&value, &start, &length) {
            Ok(result) => json_value_to_js(ctx.env, result),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("substring", substring_fn)?;

    let substring_before_fn = env.create_function_from_closure("substringBefore", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        let chars = arg_to_json_value(&ctx, 1)?;
        match strings_impl::substring_before(&value, &chars) {
            Ok(result) => json_value_to_js(ctx.env, result),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("substringBefore", substring_before_fn)?;

    let substring_after_fn = env.create_function_from_closure("substringAfter", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        let chars = arg_to_json_value(&ctx, 1)?;
        match strings_impl::substring_after(&value, &chars) {
            Ok(result) => json_value_to_js(ctx.env, result),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("substringAfter", substring_after_fn)?;

    let lowercase_fn = env.create_function_from_closure("lowercase", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        match strings_impl::lowercase(&value) {
            Ok(result) => json_value_to_js(ctx.env, result),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("lowercase", lowercase_fn)?;

    let uppercase_fn = env.create_function_from_closure("uppercase", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        match strings_impl::uppercase(&value) {
            Ok(result) => json_value_to_js(ctx.env, result),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("uppercase", uppercase_fn)?;

    let length_fn = env.create_function_from_closure("length", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        match strings_impl::length(&value) {
            Ok(result) => json_value_to_js(ctx.env, result),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("length", length_fn)?;

    let trim_fn = env.create_function_from_closure("trim", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        match strings_impl::trim(&value) {
            Ok(result) => json_value_to_js(ctx.env, result),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("trim", trim_fn)?;

    let pad_fn = env.create_function_from_closure("pad", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        let width = arg_to_json_value(&ctx, 1)?;
        let char_value = arg_to_json_value(&ctx, 2)?;
        match strings_impl::pad(&value, &width, &char_value) {
            Ok(result) => json_value_to_js(ctx.env, result),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("pad", pad_fn)?;

    Ok(())
}

fn register_unimplemented(env: &Env, exports: &mut JsObject) -> napi::Result<()> {
    const UNIMPLEMENTED: &[&str] = &[
        "count",
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
    register_core(&env, &mut exports)?;
    register_strings(&env, &mut exports)?;
    register_unimplemented(&env, &mut exports)?;
    Ok(exports)
}
