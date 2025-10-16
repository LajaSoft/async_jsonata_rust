use napi::bindgen_prelude::*;
use napi::{Env, Status, ValueType};
use napi::sys;
use std::ptr;
use jsonata_rust::functions::{core as core_impl, math as math_impl, strings as strings_impl};
use jsonata_rust::types::{JsonValue, JsonError};
use jsonata_rust::JsonataValue;

// Helper функции, перенесённые из lib.rs
fn undefined(env: &Env) -> napi::Result<Unknown<'_>> {
    ().into_unknown(env)
}

fn option_number_to_js(env: &Env, value: Option<f64>) -> napi::Result<Unknown<'_>> {
    match value {
        Some(num) => env.create_double(num).and_then(|n| n.into_unknown(env)),
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
    // Конвертируем через JsonataValue для совместимости
    let jsonata_val = arg_to_jsonata_value(ctx, index)?;
    Ok(crate::conversion::jsonata_value_to_json_value(jsonata_val))
}

fn json_value_to_js(env: &Env, value: JsonValue) -> napi::Result<Unknown<'_>> {
    // Конвертируем через JsonataValue для совместимости
    let jsonata_val = crate::conversion::json_value_to_jsonata_value(value);
    crate::conversion::jsonata_value_to_js(env, jsonata_val)
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
                        // Получаем JsonataValue, конвертируем в JsonValue, вызываем функцию, конвертируем обратно
                        let jsonata_value = arg_to_jsonata_value(&ctx, 0)?;
                        let json_value = crate::conversion::jsonata_value_to_json_value(jsonata_value);
                        let result = $impl_fn(&json_value);
                        let jsonata_result = crate::conversion::json_value_to_jsonata_value(result);
                        map_unknown(crate::conversion::jsonata_value_to_js(ctx.env, jsonata_result))
                    }
                )?;
                exports.set_named_property(stringify!($name), func)?;
                Ok(())
            }
        }
    };
}

// Создаём функции с помощью макросов
create_math_function!(sum, math_impl::sum);
create_math_function!(max, math_impl::max);
create_math_function!(min, math_impl::min);
create_math_function!(average, math_impl::average);

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
];

pub fn register_all_functions(env: &Env, exports: &mut Object) -> napi::Result<()> {
    // Регистрируем математические функции
    for (name, register_fn) in MATH_FUNCTIONS {
        register_fn(env, exports)?;
    }
    
    // Регистрируем core функции  
    for (name, register_fn) in CORE_FUNCTIONS {
        register_fn(env, exports)?;
    }
    
    Ok(())
}