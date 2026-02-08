use napi::bindgen_prelude::*;
use napi::{Env, Status, ValueType};
use napi::sys;
use jsonata_rust::functions::{core as core_impl, math as math_impl, strings as strings_impl};
use jsonata_rust::types::JsonValue;
use jsonata_rust::JsonataValue;
use crate::json_error_to_napi;

// Helper функции, перенесённые из lib.rs
fn undefined(env: &Env) -> napi::Result<Unknown<'_>> {
    ().into_unknown(env)
}

fn option_number_to_js(env: &Env, value: Option<f64>) -> napi::Result<Unknown<'_>> {
    match value {
        Some(num) => {
            let normalized = math_impl::normalize_js_number(num);
            env.create_double(normalized).and_then(|n| n.into_unknown(env))
        }
        None => undefined(env),
    }
}

fn map_unknown(result: napi::Result<Unknown>) -> napi::Result<sys::napi_value> {
    result.map(|value| value.raw())
}

fn get_number_arg(ctx: &FunctionCallContext, index: usize) -> napi::Result<Option<f64>> {
    if index >= ctx.length() {
        return Ok(None);
    }
    let value: Unknown = ctx.get(index)?;
    if matches!(value.get_type()?, ValueType::Undefined) {
        return Ok(None);
    }
    let coerced = value.coerce_to_number()?.get_double()?;
    Ok(Some(coerced))
}

fn extract_numeric_args(ctx: &FunctionCallContext) -> napi::Result<Option<Vec<f64>>> {
    if ctx.length() == 0 {
        return Ok(None);
    }
    let first: Unknown = ctx.get(0)?;
    match first.get_type()? {
        ValueType::Undefined | ValueType::Null => Ok(None),
        ValueType::Number => {
            let value: f64 = ctx.get(0)?;
            Ok(Some(vec![value]))
        }
        ValueType::Object => {
            let object: Object = ctx.get(0)?;
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

fn arg_to_jsonata_value(ctx: &FunctionCallContext, index: usize) -> napi::Result<JsonataValue> {
    if index >= ctx.length() {
        return Ok(JsonataValue::Undefined);
    }
    let value: Unknown = ctx.get(index)?;
    crate::conversion::js_to_jsonata_value(ctx.env, value)
}

fn arg_to_json_value(ctx: &FunctionCallContext, index: usize) -> napi::Result<JsonValue> {
    if index >= ctx.length() {
        return Ok(JsonValue::Undefined);
    }
    let value: Unknown = ctx.get(index)?;
    crate::js_unknown_to_json_value(ctx.env, value)
}

fn json_value_to_js(env: &Env, value: JsonValue) -> napi::Result<Unknown<'_>> {
    crate::json_value_to_js(env, value)
}

fn register_error(env: &Env, exports: &mut Object) -> napi::Result<()> {
    let func = env.create_function_from_closure::<(), sys::napi_value, _>(
        "error",
        |ctx| {
            let message_value = arg_to_json_value(&ctx, 0)?;
            let default_message = "$error() function evaluated".to_string();

            let final_message = match core_impl::boolean(&message_value) {
                JsonValue::Bool(true) => match strings_impl::string(&message_value, false) {
                    Ok(JsonValue::String(s)) if !s.is_empty() => s,
                    Ok(_) => default_message.clone(),
                    Err(err) => return Err(json_error_to_napi(err)),
                },
                _ => default_message.clone(),
            };

            Err(Error::new(
                Status::GenericFailure,
                format!("D3137: {}", final_message),
            ))
        },
    )?;
    exports.set_named_property("error", func)?;
    Ok(())
}

fn register_append(env: &Env, exports: &mut Object) -> napi::Result<()> {
    let func = env.create_function_from_closure::<(), sys::napi_value, _>(
        "append",
        |ctx| {
            let left = arg_to_jsonata_value(&ctx, 0)?;
            let right = arg_to_jsonata_value(&ctx, 1)?;
            let result = core_impl::append_jsonata(&left, &right);
            map_unknown(crate::conversion::jsonata_value_to_js(ctx.env, result))
        },
    )?;
    exports.set_named_property("append", func)?;
    Ok(())
}

fn register_sum(env: &Env, exports: &mut Object) -> napi::Result<()> {
    let func = env.create_function_from_closure::<(), sys::napi_value, _>("sum", |ctx| {
        let arg = arg_to_jsonata_value(&ctx, 0)?;
        let result = math_impl::sum_jsonata(&arg).map_err(json_error_to_napi)?;
        map_unknown(crate::conversion::jsonata_value_to_js(ctx.env, result))
    })?;
    exports.set_named_property("sum", func)?;
    Ok(())
}

fn register_average(env: &Env, exports: &mut Object) -> napi::Result<()> {
    let func = env.create_function_from_closure::<(), sys::napi_value, _>(
        "average",
        |ctx| {
            let values = extract_numeric_args(&ctx)?;
            match math_impl::average(values.as_ref().map(|v| v.as_slice())) {
                Some(num) => {
                    let number = ctx.env.create_double(num)?;
                    map_unknown(number.into_unknown(ctx.env))
                }
                None => map_unknown(undefined(ctx.env)),
            }
        },
    )?;
    exports.set_named_property("average", func)?;
    Ok(())
}

// Макрос для создания простых математических функций
macro_rules! create_math_function {
    ($name:ident, $impl_fn:path) => {
        paste::paste! {
            fn [<register_ $name>](env: &Env, exports: &mut Object) -> napi::Result<()> {
                let func = env.create_function_from_closure::<(), sys::napi_value, _>(
                    stringify!($name), 
                    |ctx| {
                        let values = extract_numeric_args(&ctx)?;
                        map_unknown(option_number_to_js(
                            ctx.env,
                            $impl_fn(values.as_ref().map(|v| v.as_slice())),
                        ))
                    }
                )?;
                exports.set_named_property(stringify!($name), func)?;
                Ok(())
            }
        }
    };
}

// Макрос для создания функций с одним числовым аргументом
macro_rules! create_single_number_function {
    ($name:ident, $impl_fn:path) => {
        paste::paste! {
            fn [<register_ $name>](env: &Env, exports: &mut Object) -> napi::Result<()> {
                let func = env.create_function_from_closure::<(), sys::napi_value, _>(
                    stringify!($name),
                    |ctx| {
                        let value = get_number_arg(&ctx, 0)?;
                        map_unknown(option_number_to_js(ctx.env, $impl_fn(value)))
                    }
                )?;
                exports.set_named_property(stringify!($name), func)?;
                Ok(())
            }
        }
    };
}

// Макрос для создания core функций
macro_rules! create_core_function {
    ($name:ident, $impl_fn:path) => {
        paste::paste! {
            fn [<register_ $name>](env: &Env, exports: &mut Object) -> napi::Result<()> {
                let func = env.create_function_from_closure::<(), sys::napi_value, _>(
                    stringify!($name),
                    |ctx| {
                        let json_value = arg_to_json_value(&ctx, 0)?;
                        let result = $impl_fn(&json_value);
                        map_unknown(json_value_to_js(ctx.env, result))
                    }
                )?;
                exports.set_named_property(stringify!($name), func)?;
                Ok(())
            }
        }
    };
}

// Создаём функции с помощью макросов
create_math_function!(max, math_impl::max);
create_math_function!(min, math_impl::min);

create_single_number_function!(abs, math_impl::abs);
create_single_number_function!(floor, math_impl::floor);
create_single_number_function!(ceil, math_impl::ceil);

create_core_function!(exists, core_impl::exists);
create_core_function!(keys, core_impl::keys);
create_core_function!(spread, core_impl::spread);

// Список всех регистрирующих функций
type RegisterFunction = fn(&Env, &mut Object) -> napi::Result<()>;

pub const MATH_FUNCTIONS: &[(&str, RegisterFunction)] = &[
    ("sum", register_sum),
    ("max", register_max),
    ("min", register_min),
    ("average", register_average),
    ("abs", register_abs),
    ("floor", register_floor),
    ("ceil", register_ceil),
];

pub const CORE_FUNCTIONS: &[(&str, RegisterFunction)] = &[
    ("exists", register_exists),
    ("keys", register_keys),
    ("spread", register_spread),
    ("append", register_append),
    ("error", register_error),
];

pub fn register_all_functions(env: &Env, exports: &mut Object) -> napi::Result<()> {
    // Регистрируем математические функции
    for (_name, register_fn) in MATH_FUNCTIONS {
        register_fn(env, exports)?;
    }
    
    // Регистрируем core функции  
    for (_name, register_fn) in CORE_FUNCTIONS {
        register_fn(env, exports)?;
    }
    
    Ok(())
}
