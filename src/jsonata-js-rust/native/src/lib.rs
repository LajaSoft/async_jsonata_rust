use futures::channel::oneshot;
use futures::future::BoxFuture;
use napi::bindgen_prelude::*;
use napi::sys;
use napi::threadsafe_function::{
    ThreadsafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode,
};
use napi::{Env, Status, ValueType};
use napi_derive::napi;

mod function_registry;
mod conversion;

use jsonata_rust::functions::{core as core_impl, math as math_impl, strings as strings_impl};
use jsonata_rust::types::{
    CallbackHandle, FunctionContext, JsonArray, JsonCallable, JsonError, JsonFunction,
    JsonObject, JsonValue, JsonataFocus,
};
use jsonata_rust::registry;
use std::any::Any;
use std::fs::OpenOptions;
use std::io::Write as IoWrite;
use std::mem;
use std::ptr;
use std::sync::{Arc, Mutex};

type JsUnknown<'env> = Unknown<'env>;
type JsObject<'env> = Object<'env>;
type JsFunction<'env> = Function<'env>;
type CallContext<'env> = FunctionCallContext<'env>;

fn undefined(env: &Env) -> napi::Result<JsUnknown<'_>> {
    ().into_unknown(env)
}

fn null(env: &Env) -> napi::Result<JsUnknown<'_>> {
    Null.into_unknown(env)
}

fn bool_to_unknown(env: &Env, value: bool) -> napi::Result<JsUnknown<'_>> {
    value.into_unknown(env)
}

fn map_unknown(result: napi::Result<JsUnknown>) -> napi::Result<sys::napi_value> {
    result.map(|value| value.raw())
}

#[derive(Clone, Copy)]
struct JsRawValue(sys::napi_value);

impl TypeName for JsRawValue {
    fn type_name() -> &'static str {
        "unknown"
    }

    fn value_type() -> ValueType {
        ValueType::Unknown
    }
}

impl ValidateNapiValue for JsRawValue {}

impl ToNapiValue for JsRawValue {
    unsafe fn to_napi_value(_env: sys::napi_env, val: Self) -> napi::Result<sys::napi_value> {
        Ok(val.0)
    }
}

impl FromNapiValue for JsRawValue {
    unsafe fn from_napi_value(_env: sys::napi_env, value: sys::napi_value) -> napi::Result<Self> {
        Ok(JsRawValue(value))
    }
}

struct RawArgList {
    values: Vec<sys::napi_value>,
}

impl JsValuesTupleIntoVec for RawArgList {
    fn into_vec(mut self, _env: sys::napi_env) -> napi::Result<Vec<sys::napi_value>> {
        Ok(std::mem::take(&mut self.values))
    }
}

trait JsFunctionExt<'env> {
    fn try_from_unknown(value: JsUnknown<'env>) -> napi::Result<JsFunction<'env>>;
    fn call(
        &self,
        this: Option<&JsObject<'env>>,
        args: &[JsUnknown<'env>],
    ) -> napi::Result<JsUnknown<'env>>;
}

impl<'env> JsFunctionExt<'env> for JsFunction<'env> {
    fn try_from_unknown(value: JsUnknown<'env>) -> napi::Result<JsFunction<'env>> {
        if value.get_type()? != ValueType::Function {
            return Err(Error::new(
                Status::InvalidArg,
                "Value is not a function".to_owned(),
            ));
        }
        unsafe { value.cast::<JsFunction<'env>>() }
    }

    fn call(
        &self,
        this: Option<&JsObject<'env>>,
        args: &[JsUnknown<'env>],
    ) -> napi::Result<JsUnknown<'env>> {
        let env = self.value().env;
        let this_value = if let Some(this_obj) = this {
            this_obj.raw()
        } else {
            let mut undefined = ptr::null_mut();
            check_status!(unsafe { sys::napi_get_undefined(env, &mut undefined) })?;
            undefined
        };
        let mut raw_args: Vec<sys::napi_value> = Vec::with_capacity(args.len());
        for arg in args {
            raw_args.push(arg.raw());
        }
        let mut raw_result = ptr::null_mut();
        check_pending_exception!(
            env,
            unsafe {
                sys::napi_call_function(
                    env,
                    this_value,
                    self.raw(),
                    raw_args.len(),
                    raw_args.as_ptr(),
                    &mut raw_result,
                )
            }
        )?;
        unsafe { JsUnknown::from_napi_value(env, raw_result) }
    }
}

fn option_number_to_js(env: &Env, value: Option<f64>) -> napi::Result<JsUnknown<'_>> {
    match value {
        Some(num) => env.create_double(num).and_then(|n| n.into_unknown(env)),
        None => undefined(env),
    }
}

fn get_number_arg(ctx: &CallContext, index: usize) -> napi::Result<Option<f64>> {
    if index >= ctx.length() {
        return Ok(None);
    }
    let value: JsUnknown = ctx.get(index)?;
    if matches!(value.get_type()?, ValueType::Undefined) {
        return Ok(None);
    }
    let coerced = value.coerce_to_number()?.get_double()?;
    Ok(Some(coerced))
}

fn extract_numeric_args(ctx: &CallContext) -> napi::Result<Option<Vec<f64>>> {
    if ctx.length() == 0 {
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
    let sum = env.create_function_from_closure::<(), sys::napi_value, _>("sum", |ctx| {
        let values = extract_numeric_args(&ctx)?;
        map_unknown(option_number_to_js(
            ctx.env,
            math_impl::sum(values.as_ref().map(|v| v.as_slice())),
        ))
    })?;
    exports.set_named_property("sum", sum)?;

    let max = env.create_function_from_closure::<(), sys::napi_value, _>("max", |ctx| {
        let values = extract_numeric_args(&ctx)?;
        map_unknown(option_number_to_js(
            ctx.env,
            math_impl::max(values.as_ref().map(|v| v.as_slice())),
        ))
    })?;
    exports.set_named_property("max", max)?;

    let min = env.create_function_from_closure::<(), sys::napi_value, _>("min", |ctx| {
        let values = extract_numeric_args(&ctx)?;
        map_unknown(option_number_to_js(
            ctx.env,
            math_impl::min(values.as_ref().map(|v| v.as_slice())),
        ))
    })?;
    exports.set_named_property("min", min)?;

    let average = env.create_function_from_closure::<(), sys::napi_value, _>("average", |ctx| {
        let values = extract_numeric_args(&ctx)?;
        map_unknown(option_number_to_js(
            ctx.env,
            math_impl::average(values.as_ref().map(|v| v.as_slice())),
        ))
    })?;
    exports.set_named_property("average", average)?;

    let count = env.create_function_from_closure::<(), sys::napi_value, _>("count", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        map_unknown(json_value_to_js(ctx.env, math_impl::count_value(&value)))
    })?;
    exports.set_named_property("count", count)?;

    let abs = env.create_function_from_closure::<(), sys::napi_value, _>("abs", |ctx| {
        let value = get_number_arg(&ctx, 0)?;
        map_unknown(option_number_to_js(ctx.env, math_impl::abs(value)))
    })?;
    exports.set_named_property("abs", abs)?;

    let floor = env.create_function_from_closure::<(), sys::napi_value, _>("floor", |ctx| {
        let value = get_number_arg(&ctx, 0)?;
        map_unknown(option_number_to_js(ctx.env, math_impl::floor(value)))
    })?;
    exports.set_named_property("floor", floor)?;

    let ceil = env.create_function_from_closure::<(), sys::napi_value, _>("ceil", |ctx| {
        let value = get_number_arg(&ctx, 0)?;
        map_unknown(option_number_to_js(ctx.env, math_impl::ceil(value)))
    })?;
    exports.set_named_property("ceil", ceil)?;

    let round = env.create_function_from_closure::<(), sys::napi_value, _>("round", |ctx| {
        let value = get_number_arg(&ctx, 0)?;
        let precision = get_number_arg(&ctx, 1)?;
        map_unknown(option_number_to_js(ctx.env, math_impl::round(value, precision)))
    })?;
    exports.set_named_property("round", round)?;

    let sqrt = env.create_function_from_closure::<(), sys::napi_value, _>("sqrt", |ctx| {
        let value = get_number_arg(&ctx, 0)?;
        match math_impl::sqrt(value) {
            Ok(result) => map_unknown(option_number_to_js(ctx.env, result)),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("sqrt", sqrt)?;

    let power = env.create_function_from_closure::<(), sys::napi_value, _>("power", |ctx| {
        let base = get_number_arg(&ctx, 0)?;
        let exponent = get_number_arg(&ctx, 1)?;
        match math_impl::power(base, exponent) {
            Ok(result) => map_unknown(option_number_to_js(ctx.env, result)),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("power", power)?;

    let number_fn = env.create_function_from_closure::<(), sys::napi_value, _>("number", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        match math_impl::number(&value) {
            Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("number", number_fn)?;

    let random = env.create_function_from_closure::<(), sys::napi_value, _>("random", |ctx| {
        let value = math_impl::random();
        map_unknown(
            ctx.env
                .create_double(value)
                .and_then(|v| v.into_unknown(ctx.env)),
        )
    })?;
    exports.set_named_property("random", random)?;

    Ok(())
}

pub(crate) fn js_unknown_to_json_value(env: &Env, value: JsUnknown) -> napi::Result<JsonValue> {
    match value.get_type()? {
        ValueType::Undefined => Ok(JsonValue::Undefined),
        ValueType::Null => Ok(JsonValue::Null),
        ValueType::Boolean => Ok(JsonValue::Bool(value.coerce_to_bool()?)),
        ValueType::Number => Ok(JsonValue::Number(value.coerce_to_number()?.get_double()?)),
        ValueType::String => Ok(JsonValue::String(
            value.coerce_to_string()?.into_utf8()?.as_str()?.to_owned(),
        )),
        ValueType::BigInt => Ok(JsonValue::String(
            value.coerce_to_string()?.into_utf8()?.as_str()?.to_owned(),
        )),
        ValueType::Function => {
            let js_func = JsFunction::try_from_unknown(value)?;
            
            // Check if this is a built-in Rust function 
            if let Ok(js_obj) = js_func.coerce_to_object() {
                // First try _rustBuiltin marker
                if let Ok(builtin_name_value) = js_obj.get_named_property::<JsUnknown>("_rustBuiltin") {
                    if let Ok(name_str) = builtin_name_value.coerce_to_string() {
                        if let Ok(utf8_str) = name_str.into_utf8() {
                            if let Ok(name) = utf8_str.as_str() {
                                eprintln!("[DEBUG] Found _rustBuiltin marker for: '{}'", name);
                                
                                // Only handle if the name is not empty/undefined
                                if !name.is_empty() && name != "undefined" {
                                    // Try to get built-in function
                                    if let Some(builtin) = registry::lookup_builtin(name) {
                                        eprintln!("[DEBUG] Using built-in function for: {}", name);
                                        return Ok(JsonValue::Function(builtin));
                                    }
                                }
                            }
                        }
                    }
                }
                
                // Second try: check toString() for [native code] signature
                if let Ok(to_string_fn) = js_obj.get_named_property::<JsFunction>("toString") {
                    if let Ok(to_string_result) = JsFunctionExt::call(&to_string_fn, Some(&js_obj), &[]) {
                        if let Ok(string_result) = to_string_result.coerce_to_string() {
                            if let Ok(utf8_result) = string_result.into_utf8() {
                                if let Ok(string_content) = utf8_result.as_str() {
                                    eprintln!("[DEBUG] Function toString(): '{}'", string_content);
                                    // Look for our custom [native code] pattern and extract function name
                                    if string_content.contains("[native code]") {
                                        // Extract function name from patterns like "function boolean(...) { [native code] }"
                                        if let Some(start) = string_content.find("function ") {
                                            let after_function = &string_content[start + 9..];
                                            if let Some(end) = after_function.find('(') {
                                                let func_name = after_function[..end].trim();
                                                if !func_name.is_empty() {
                                                    eprintln!("[DEBUG] Extracted function name from toString: '{}'", func_name);
                                                    if let Some(builtin) = registry::lookup_builtin(func_name) {
                                                        eprintln!("[DEBUG] Using built-in function for toString name: {}", func_name);
                                                        return Ok(JsonValue::Function(builtin));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                
                // Third try: look for _rustImpl property that contains the original function
                if let Ok(impl_value) = js_obj.get_named_property::<JsUnknown>("_rustImpl") {
                    if impl_value.get_type()? == ValueType::Function {
                        eprintln!("[DEBUG] Found _rustImpl property, checking recursively");
                        // Recursively check the impl function
                        let impl_result = js_unknown_to_json_value(env, impl_value)?;
                        if let JsonValue::Function(_) = impl_result {
                            return Ok(impl_result);
                        }
                    }
                }
                
                // Fourth try: function name property
                if let Ok(name_value) = js_obj.get_named_property::<JsUnknown>("name") {
                    if let Ok(name_str) = name_value.coerce_to_string() {
                        if let Ok(utf8_str) = name_str.into_utf8() {
                            if let Ok(name) = utf8_str.as_str() {
                                eprintln!("[DEBUG] Checking function name: '{}'", name);
                                
                                // Check if it's a known built-in
                                if let Some(builtin) = registry::lookup_builtin(name) {
                                    eprintln!("[DEBUG] Using built-in function for name: {}", name);
                                    return Ok(JsonValue::Function(builtin));
                                }
                            }
                        }
                    }
                }
            }
            
            // Fallback to JS callable
            eprintln!("[DEBUG] Using JS callable for function");
            let callable = JsFunctionCallable::new(env, js_func)?;
            Ok(JsonValue::Function(JsonFunction::new(Arc::new(callable))))
        }
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
                        ValueType::Boolean => flag.coerce_to_bool()?,
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
                        ValueType::Boolean => flag.coerce_to_bool()?,
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
                if is_jsonata_function_object(&object)? {
                    if object.has_named_property("apply")? {
                        let apply_value: JsUnknown = object.get_named_property("apply")?;
                        if matches!(apply_value.get_type()?, ValueType::Function) {
                            return js_unknown_to_json_value(env, apply_value);
                        }
                    }
                }

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

pub(crate) fn json_value_to_js(env: &Env, value: JsonValue) -> napi::Result<JsUnknown<'_>> {
    match value {
        JsonValue::Undefined => undefined(env),
        JsonValue::Null => null(env),
        JsonValue::Bool(flag) => bool_to_unknown(env, flag),
        JsonValue::Number(num) => env.create_double(num).and_then(|v| v.into_unknown(env)),
        JsonValue::String(text) => env.create_string(&text).and_then(|v| v.into_unknown(env)),
        JsonValue::Array(array) => {
            let mut js_array = env
                .create_array(array.elements.len() as u32)?
                .coerce_to_object()?;
            for (index, element) in array.elements.into_iter().enumerate() {
                let js_value = json_value_to_js(env, element)?;
                js_array.set_element(index as u32, js_value)?;
            }
            if array.is_sequence {
                js_array.set_named_property("sequence", true)?;
            }
            if array.outer_wrapper {
                js_array.set_named_property("outerWrapper", true)?;
            }
            Ok(js_array.to_unknown())
        }
        JsonValue::Object(JsonObject(entries)) => {
            let mut js_object = Object::new(env)?;
            for (key, entry_value) in entries {
                let js_value = json_value_to_js(env, entry_value)?;
                js_object.set_named_property(&key, js_value)?;
            }
            Ok(js_object.to_unknown())
        }
        JsonValue::Function(function) => {
            let callable = function.as_callable();
            if let Some(js_callable) = callable.as_any().downcast_ref::<JsFunctionCallable>() {
                let js_func = js_callable.to_js_function(env)?;
                js_func.into_unknown(env)
            } else {
                Err(Error::new(
                    Status::InvalidArg,
                    "Cannot convert function value to JavaScript",
                ))
            }
        }
    }
}

fn property_flag_is_truthy(object: &JsObject, name: &str) -> napi::Result<bool> {
    if !object.has_named_property(name)? {
        return Ok(false);
    }
    let flag: JsUnknown = object.get_named_property(name)?;
    match flag.get_type()? {
        ValueType::Undefined | ValueType::Null => Ok(false),
        ValueType::Boolean => flag.coerce_to_bool(),
        ValueType::Number => Ok(flag.coerce_to_number()?.get_double()? != 0.0),
        ValueType::String => {
            let text = flag.coerce_to_string()?.into_utf8()?.as_str()?.to_owned();
            Ok(!text.is_empty() && text != "false" && text != "0")
        }
        _ => Ok(true),
    }
}

fn is_jsonata_function_object(object: &JsObject) -> napi::Result<bool> {
    if property_flag_is_truthy(object, "_jsonata_function")? {
        return Ok(true);
    }
    if property_flag_is_truthy(object, "_jsonata_lambda")? {
        return Ok(true);
    }
    Ok(false)
}

fn create_sequence_array<'env>(
    env: &'env Env,
    elements: Vec<JsUnknown<'env>>,
) -> napi::Result<JsUnknown<'env>> {
    let mut array = env.create_array(elements.len() as u32)?;
    for (index, element) in elements.into_iter().enumerate() {
        array.set(index as u32, element)?;
    }
    let mut js_array = array.coerce_to_object()?;
    js_array.set_named_property("sequence", true)?;
    Ok(js_array.to_unknown())
}

fn lookup_js<'env>(
    env: &'env Env,
    value: JsUnknown<'env>,
    key: &str,
) -> napi::Result<Option<JsUnknown<'env>>> {
    match value.get_type()? {
        ValueType::Undefined | ValueType::Null => Ok(None),
        ValueType::Function => Ok(None),
        ValueType::Object => {
            let object = value.coerce_to_object()?;
            if object.is_array()? {
                let length = object.get_array_length()?;
                let mut aggregated: Vec<JsUnknown> = Vec::new();
                for index in 0..length {
                    let element: JsUnknown = object.get_element(index)?;
                    if let Some(resolved) = lookup_js(env, element, key)? {
                        match resolved.get_type()? {
                            ValueType::Undefined | ValueType::Null => {}
                            ValueType::Object => {
                                let resolved_object = resolved.coerce_to_object()?;
                                if resolved_object.is_array()? {
                                    let inner_length = resolved_object.get_array_length()?;
                                    for inner_index in 0..inner_length {
                                        let inner_value: JsUnknown =
                                            resolved_object.get_element(inner_index)?;
                                        aggregated.push(inner_value);
                                    }
                                } else {
                                    aggregated.push(resolved_object.to_unknown());
                                }
                            }
                            _ => aggregated.push(resolved),
                        }
                    }
                }
                let sequence = create_sequence_array(env, aggregated)?;
                Ok(Some(sequence))
            } else {
                if is_jsonata_function_object(&object)? {
                    return Ok(None);
                }
                if object.has_named_property(key)? {
                    let property: JsUnknown = object.get_named_property(key)?;
                    if matches!(property.get_type()?, ValueType::Undefined) {
                        return Ok(None);
                    }
                    return Ok(Some(property));
                }
                Ok(None)
            }
        }
        _ => Ok(None),
    }
}

fn arg_to_json_value(ctx: &CallContext, index: usize) -> napi::Result<JsonValue> {
    if index >= ctx.length() {
        return Ok(JsonValue::Undefined);
    }
    let value: JsUnknown = ctx.get(index)?;
    js_unknown_to_json_value(ctx.env, value)
}

fn function_context_from_this(ctx: &CallContext) -> napi::Result<FunctionContext> {
    let this_value = ctx.this::<JsUnknown>()?;
    match this_value.get_type()? {
        ValueType::Undefined | ValueType::Null => Ok(FunctionContext::empty()),
        _ => {
            let handle = Arc::new(JsThisHandle::new(ctx.env.raw(), &this_value)?);
            let focus_value = match handle.to_js_unknown(ctx.env) {
                Ok(js_this) => js_unknown_to_json_value(ctx.env, js_this).unwrap_or(JsonValue::Undefined),
                Err(_) => JsonValue::Undefined,
            };
            let callback_handle: CallbackHandle = handle.clone();
            Ok(FunctionContext::with_focus(JsonataFocus::with_handle(
                focus_value,
                Some(callback_handle),
            )))
        }
    }
}

pub(crate) fn json_error_to_napi(err: JsonError) -> napi::Error {
    eprintln!("[DEBUG] json_error_to_napi: code={}, message={}", err.code, err.message);
    napi::Error::new(
        Status::GenericFailure,
        format!("{}: {}", err.code, err.message),
    )
}

fn napi_error_to_json(code: &'static str, err: napi::Error) -> JsonError {
    let message = err.to_string();
    eprintln!("[DEBUG] napi_error_to_json: code={}, message={}", code, message);
    
    // Try to extract useful information if the error message is not helpful
    if message == "[object Object]" || message.is_empty() {
        let reason = match err.status {
            Status::InvalidArg => "Invalid argument",
            Status::ObjectExpected => "Object expected", 
            Status::StringExpected => "String expected",
            Status::FunctionExpected => "Function expected",
            Status::NumberExpected => "Number expected",
            Status::BooleanExpected => "Boolean expected",
            Status::ArrayExpected => "Array expected",
            Status::GenericFailure => "Generic failure",
            _ => "Unknown napi error",
        };
        JsonError::new(code, format!("{} (status: {:?})", reason, err.status))
    } else {
        JsonError::new(code, message)
    }
}

type JsonCallResult = std::result::Result<JsonValue, JsonError>;

struct JsThisHandle {
    env: sys::napi_env,
    reference: sys::napi_ref,
}

unsafe impl Send for JsThisHandle {}
unsafe impl Sync for JsThisHandle {}

impl JsThisHandle {
    fn new(env: sys::napi_env, value: &JsUnknown) -> napi::Result<Self> {
        let mut reference = std::ptr::null_mut();
        let raw_value = value.raw();
        let status = unsafe { sys::napi_create_reference(env, raw_value, 1, &mut reference) };
        if status != sys::Status::napi_ok {
            return Err(Error::from_status(Status::from(status)));
        }
        Ok(Self { env, reference })
    }

    fn to_js_unknown(&self, env: &Env) -> napi::Result<JsUnknown<'_>> {
        let mut value = std::ptr::null_mut();
        let status = unsafe { sys::napi_get_reference_value(env.raw(), self.reference, &mut value) };
        if status != sys::Status::napi_ok {
            return Err(Error::from_status(Status::from(status)));
        }
        unsafe { Ok(JsUnknown::from_raw_unchecked(env.raw(), value)) }
    }
}

impl Drop for JsThisHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = sys::napi_delete_reference(self.env, self.reference);
        }
    }
}

struct JsFunctionHandle {
    env: sys::napi_env,
    reference: sys::napi_ref,
}

unsafe impl Send for JsFunctionHandle {}
unsafe impl Sync for JsFunctionHandle {}

impl JsFunctionHandle {
    fn new(env: sys::napi_env, func: &JsFunction) -> napi::Result<Self> {
        let mut reference = std::ptr::null_mut();
        let raw_func = func.raw();
        let status = unsafe { sys::napi_create_reference(env, raw_func, 1, &mut reference) };
        if status != sys::Status::napi_ok {
            return Err(Error::from_status(Status::from(status)));
        }
        Ok(Self { env, reference })
    }

    fn to_js_function(&self, env: &Env) -> napi::Result<JsFunction<'_>> {
        let mut value = std::ptr::null_mut();
        let status = unsafe { sys::napi_get_reference_value(env.raw(), self.reference, &mut value) };
        if status != sys::Status::napi_ok {
            return Err(Error::from_status(Status::from(status)));
        }
        unsafe { JsFunction::from_napi_value(env.raw(), value) }
    }
}

impl Drop for JsFunctionHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = sys::napi_delete_reference(self.env, self.reference);
        }
    }
}

struct SharedSender {
    inner: Arc<Mutex<Option<oneshot::Sender<JsonCallResult>>>>,
}

impl Clone for SharedSender {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl SharedSender {
    fn new(sender: oneshot::Sender<JsonCallResult>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Some(sender))),
        }
    }

    fn take(&self) -> Option<oneshot::Sender<JsonCallResult>> {
        self.inner.lock().ok().and_then(|mut guard| guard.take())
    }

    fn send(&self, result: JsonCallResult) {
        if let Some(tx) = self.take() {
            let _ = tx.send(result);
        }
    }
}

fn register_core(env: &Env, exports: &mut JsObject) -> napi::Result<()> {
    let lookup = env.create_function_from_closure::<(), sys::napi_value, _>("lookup", |ctx| {
        if ctx.length() < 2 {
            return map_unknown(undefined(ctx.env));
        }
        let key: String = ctx.get(1)?;
        let input: JsUnknown = ctx.get(0)?;
        if let Some(value) = lookup_js(ctx.env, input, &key)? {
            Ok(value.raw())
        } else {
            map_unknown(undefined(ctx.env))
        }
    })?;
    exports.set_named_property("lookup", lookup)?;

    let append = env.create_function_from_closure::<(), sys::napi_value, _>("append", |ctx| {
        let left = arg_to_json_value(&ctx, 0)?;
        let right = arg_to_json_value(&ctx, 1)?;
        let result = core_impl::append(&left, &right);
        map_unknown(json_value_to_js(ctx.env, result))
    })?;
    exports.set_named_property("append", append)?;

    let exists = env.create_function_from_closure::<(), sys::napi_value, _>("exists", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        let result = core_impl::exists(&value);
        map_unknown(json_value_to_js(ctx.env, result))
    })?;
    exports.set_named_property("exists", exists)?;

    let zip_fn = env.create_function_from_closure::<(), sys::napi_value, _>("zip", |ctx| {
        let mut values: Vec<JsonValue> = Vec::with_capacity(ctx.length());
        for index in 0..ctx.length() {
            values.push(arg_to_json_value(&ctx, index)?);
        }
        let result = core_impl::zip(&values);
        map_unknown(json_value_to_js(ctx.env, result))
    })?;
    exports.set_named_property("zip", zip_fn)?;

    let keys = env.create_function_from_closure::<(), sys::napi_value, _>("keys", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        let result = core_impl::keys(&value);
        map_unknown(json_value_to_js(ctx.env, result))
    })?;
    exports.set_named_property("keys", keys)?;

    let boolean_fn = env.create_function_from_closure::<(), sys::napi_value, _>("boolean", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        let result = core_impl::boolean(&value);
        map_unknown(json_value_to_js(ctx.env, result))
    })?;
    exports.set_named_property("boolean", boolean_fn)?;

    let type_fn = env.create_function_from_closure::<(), sys::napi_value, _>("type", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        let result = core_impl::type_of(&value);
        map_unknown(json_value_to_js(ctx.env, result))
    })?;
    exports.set_named_property("type", type_fn)?;

    let not_fn = env.create_function_from_closure::<(), sys::napi_value, _>("not", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        let result = core_impl::not(&value);
        map_unknown(json_value_to_js(ctx.env, result))
    })?;
    exports.set_named_property("not", not_fn)?;

    let map_fn = env.create_function_from_closure::<(), sys::napi_value, _>("map", |ctx| {
        let array = arg_to_json_value(&ctx, 0)?;
        let func = arg_to_json_value(&ctx, 1)?;
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open("/workspace/tmp/jsonata_map_debug.log")
        {
            match &array {
                JsonValue::Array(arr) => {
                    let _ = writeln!(
                        file,
                        "map input len={} elements={:?}",
                        arr.elements.len(),
                        arr.elements
                    );
                }
                other => {
                    let _ = writeln!(file, "map input non-array: {:?}", other);
                }
            }
        }
        let focus = function_context_from_this(&ctx)?;
        ctx.env
            .spawn_future_with_callback(
                async move {
                    core_impl::map(focus, array, func)
                        .await
                        .map_err(json_error_to_napi)
                },
                |env, result| json_value_to_js(env, result),
            )
            .map(|promise| promise.raw())
    })?;
    exports.set_named_property("map", map_fn)?;

    let filter_fn = env.create_function_from_closure::<(), sys::napi_value, _>("filter", |ctx| {
        let array = arg_to_json_value(&ctx, 0)?;
        let func = arg_to_json_value(&ctx, 1)?;
        let focus = function_context_from_this(&ctx)?;
        ctx.env
            .spawn_future_with_callback(
                async move {
                    core_impl::filter(focus, array, func)
                        .await
                        .map_err(json_error_to_napi)
                },
                |env, result| json_value_to_js(env, result),
            )
            .map(|promise| promise.raw())
    })?;
    exports.set_named_property("filter", filter_fn)?;

    let single_fn = env.create_function_from_closure::<(), sys::napi_value, _>("single", |ctx| {
        let array = arg_to_json_value(&ctx, 0)?;
        let func = arg_to_json_value(&ctx, 1)?;
        let focus = function_context_from_this(&ctx)?;
        ctx.env
            .spawn_future_with_callback(
                async move {
                    core_impl::single(focus, array, func)
                        .await
                        .map_err(json_error_to_napi)
                },
                |env, result| json_value_to_js(env, result),
            )
            .map(|promise| promise.raw())
    })?;
    exports.set_named_property("single", single_fn)?;

    let fold_left_fn =
        env.create_function_from_closure::<(), sys::napi_value, _>("foldLeft", |ctx| {
            let sequence = arg_to_json_value(&ctx, 0)?;
            let func = arg_to_json_value(&ctx, 1)?;
            let init = arg_to_json_value(&ctx, 2)?;
            let focus = function_context_from_this(&ctx)?;
            ctx.env
                .spawn_future_with_callback(
                    async move {
                        core_impl::fold_left(focus, sequence, func, init)
                            .await
                            .map_err(json_error_to_napi)
                    },
                    |env, result| json_value_to_js(env, result),
                )
                .map(|promise| promise.raw())
        })?;
    exports.set_named_property("foldLeft", fold_left_fn)?;

    let sift_fn = env.create_function_from_closure::<(), sys::napi_value, _>("sift", |ctx| {
        let input = arg_to_json_value(&ctx, 0)?;
        let func = arg_to_json_value(&ctx, 1)?;
        let focus = function_context_from_this(&ctx)?;
        ctx.env
            .spawn_future_with_callback(
                async move {
                    core_impl::sift(focus, input, func)
                        .await
                        .map_err(json_error_to_napi)
                },
                |env, result| json_value_to_js(env, result),
            )
            .map(|promise| promise.raw())
    })?;
    exports.set_named_property("sift", sift_fn)?;

    let each_fn = env.create_function_from_closure::<(), sys::napi_value, _>("each", |ctx| {
        let input = arg_to_json_value(&ctx, 0)?;
        let func = arg_to_json_value(&ctx, 1)?;
        let focus = function_context_from_this(&ctx)?;
        ctx.env
            .spawn_future_with_callback(
                async move {
                    core_impl::each(focus, input, func)
                        .await
                        .map_err(json_error_to_napi)
                },
                |env, result| json_value_to_js(env, result),
            )
            .map(|promise| promise.raw())
    })?;
    exports.set_named_property("each", each_fn)?;

    let sort_fn = env.create_function_from_closure::<(), sys::napi_value, _>("sort", |ctx| {
        let array = arg_to_json_value(&ctx, 0)?;
        if ctx.length() > 1 {
            let comparator: JsUnknown = ctx.get(1)?;
            match comparator.get_type()? {
                ValueType::Undefined | ValueType::Null => {}
                ValueType::Function => {
                    return Err(Error::new(
                        Status::GenericFailure,
                        "Rust sort does not yet support comparator functions",
                    ));
                }
                _ => {
                    return Err(Error::new(
                        Status::GenericFailure,
                        "Comparator must be a function",
                    ));
                }
            }
        }
        match core_impl::sort_default(&array) {
            Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("sort", sort_fn)?;

    let spread_fn = env.create_function_from_closure::<(), sys::napi_value, _>("spread", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        let result = core_impl::spread(&value);
        map_unknown(json_value_to_js(ctx.env, result))
    })?;
    exports.set_named_property("spread", spread_fn)?;

    let merge_fn = env.create_function_from_closure::<(), sys::napi_value, _>("merge", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        match core_impl::merge(&value) {
            Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("merge", merge_fn)?;

    let reverse_fn =
        env.create_function_from_closure::<(), sys::napi_value, _>("reverse", |ctx| {
            let value = arg_to_json_value(&ctx, 0)?;
            match core_impl::reverse(&value) {
                Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
                Err(err) => Err(json_error_to_napi(err)),
            }
        })?;
    exports.set_named_property("reverse", reverse_fn)?;

    let shuffle_fn =
        env.create_function_from_closure::<(), sys::napi_value, _>("shuffle", |ctx| {
            let value = arg_to_json_value(&ctx, 0)?;
            match core_impl::shuffle(&value) {
                Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
                Err(err) => Err(json_error_to_napi(err)),
            }
        })?;
    exports.set_named_property("shuffle", shuffle_fn)?;

    let distinct_fn =
        env.create_function_from_closure::<(), sys::napi_value, _>("distinct", |ctx| {
            let value = arg_to_json_value(&ctx, 0)?;
            match core_impl::distinct(&value) {
                Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
                Err(err) => Err(json_error_to_napi(err)),
            }
        })?;
    exports.set_named_property("distinct", distinct_fn)?;

    let assert_fn = env.create_function_from_closure::<(), sys::napi_value, _>("assert", |ctx| {
        let condition = arg_to_json_value(&ctx, 0)?;
        let message_value = arg_to_json_value(&ctx, 1)?;
        let message_ref = if matches!(message_value, JsonValue::Undefined) {
            None
        } else {
            Some(&message_value)
        };
        match core_impl::assert(&condition, message_ref) {
            Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("assert", assert_fn)?;

    Ok(())
}

fn register_strings(env: &Env, exports: &mut JsObject) -> napi::Result<()> {
    let string_fn = env.create_function_from_closure::<(), sys::napi_value, _>("string", |ctx| {
        if ctx.length() == 0 {
            return map_unknown(undefined(ctx.env));
        }

        let first: JsUnknown = ctx.get(0)?;
        match first.get_type()? {
            ValueType::Undefined => return map_unknown(undefined(ctx.env)),
            ValueType::Function => {
                let js_string = ctx.env.create_string("")?;
                return map_unknown(js_string.into_unknown(ctx.env));
            }
            _ => {}
        }

        let prettify = if ctx.length() > 1 {
            let flag: JsUnknown = ctx.get(1)?;
            flag.coerce_to_bool()?
        } else {
            false
        };

        let value = js_unknown_to_json_value(ctx.env, first)?;
        match strings_impl::string(&value, prettify) {
            Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;

    exports.set_named_property("string", string_fn)?;

    let base64encode_fn =
        env.create_function_from_closure::<(), sys::napi_value, _>("base64encode", |ctx| {
            let value = arg_to_json_value(&ctx, 0)?;
            match strings_impl::base64encode(&value) {
                Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
                Err(err) => Err(json_error_to_napi(err)),
            }
        })?;
    exports.set_named_property("base64encode", base64encode_fn)?;

    let base64decode_fn =
        env.create_function_from_closure::<(), sys::napi_value, _>("base64decode", |ctx| {
            let value = arg_to_json_value(&ctx, 0)?;
            match strings_impl::base64decode(&value) {
                Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
                Err(err) => Err(json_error_to_napi(err)),
            }
        })?;
    exports.set_named_property("base64decode", base64decode_fn)?;

    let encode_component_fn = env
        .create_function_from_closure::<(), sys::napi_value, _>("encodeUrlComponent", |ctx| {
            let value = arg_to_json_value(&ctx, 0)?;
            match strings_impl::encode_url_component(&value) {
                Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
                Err(err) => Err(json_error_to_napi(err)),
            }
        })?;
    exports.set_named_property("encodeUrlComponent", encode_component_fn)?;

    let encode_url_fn =
        env.create_function_from_closure::<(), sys::napi_value, _>("encodeUrl", |ctx| {
            let value = arg_to_json_value(&ctx, 0)?;
            match strings_impl::encode_url(&value) {
                Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
                Err(err) => Err(json_error_to_napi(err)),
            }
        })?;
    exports.set_named_property("encodeUrl", encode_url_fn)?;

    let decode_component_fn = env
        .create_function_from_closure::<(), sys::napi_value, _>("decodeUrlComponent", |ctx| {
            let value = arg_to_json_value(&ctx, 0)?;
            match strings_impl::decode_url_component(&value) {
                Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
                Err(err) => Err(json_error_to_napi(err)),
            }
        })?;
    exports.set_named_property("decodeUrlComponent", decode_component_fn)?;

    let decode_url_fn =
        env.create_function_from_closure::<(), sys::napi_value, _>("decodeUrl", |ctx| {
            let value = arg_to_json_value(&ctx, 0)?;
            match strings_impl::decode_url(&value) {
                Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
                Err(err) => Err(json_error_to_napi(err)),
            }
        })?;
    exports.set_named_property("decodeUrl", decode_url_fn)?;

    let substring_fn = env.create_function_from_closure::<(), sys::napi_value, _>("substring", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        let start = arg_to_json_value(&ctx, 1)?;
        let length = arg_to_json_value(&ctx, 2)?;
        match strings_impl::substring(&value, &start, &length) {
            Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("substring", substring_fn)?;

    let substring_before_fn = env.create_function_from_closure::<(), sys::napi_value, _>(
        "substringBefore",
        |ctx| {
            let value = arg_to_json_value(&ctx, 0)?;
            let chars = arg_to_json_value(&ctx, 1)?;
            match strings_impl::substring_before(&value, &chars) {
                Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
                Err(err) => Err(json_error_to_napi(err)),
            }
        },
    )?;
    exports.set_named_property("substringBefore", substring_before_fn)?;

    let substring_after_fn = env.create_function_from_closure::<(), sys::napi_value, _>(
        "substringAfter",
        |ctx| {
            let value = arg_to_json_value(&ctx, 0)?;
            let chars = arg_to_json_value(&ctx, 1)?;
            match strings_impl::substring_after(&value, &chars) {
                Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
                Err(err) => Err(json_error_to_napi(err)),
            }
        },
    )?;
    exports.set_named_property("substringAfter", substring_after_fn)?;

    let lowercase_fn = env.create_function_from_closure::<(), sys::napi_value, _>("lowercase", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        match strings_impl::lowercase(&value) {
            Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("lowercase", lowercase_fn)?;

    let uppercase_fn = env.create_function_from_closure::<(), sys::napi_value, _>("uppercase", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        match strings_impl::uppercase(&value) {
            Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("uppercase", uppercase_fn)?;

    let length_fn = env.create_function_from_closure::<(), sys::napi_value, _>("length", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        match strings_impl::length(&value) {
            Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("length", length_fn)?;

    let trim_fn = env.create_function_from_closure::<(), sys::napi_value, _>("trim", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        match strings_impl::trim(&value) {
            Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("trim", trim_fn)?;

    let pad_fn = env.create_function_from_closure::<(), sys::napi_value, _>("pad", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        let width = arg_to_json_value(&ctx, 1)?;
        let char_value = arg_to_json_value(&ctx, 2)?;
        match strings_impl::pad(&value, &width, &char_value) {
            Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("pad", pad_fn)?;

    Ok(())
}

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
    
    // Используем новый registry
    function_registry::register_all_functions(&env, &mut exports)?;
    
    // Пока оставляем старые функции для совместимости
    register_strings(&env, &mut exports)?;
    register_unimplemented(&env, &mut exports)?;
    
    let unknown = exports.to_unknown();
    // SAFETY: the returned value is tied to the lifetime of the Node environment, which
    // outlives this call as the addon remains loaded.
    Ok(unsafe { mem::transmute::<JsUnknown<'_>, JsUnknown<'static>>(unknown) })
}

#[napi(js_name = "parseExpression")]
pub fn parse_expression(env: Env, source: String, recover: Option<bool>) -> napi::Result<JsUnknown<'static>> {
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

struct JsFunctionCallable {
    tsfn: ThreadsafeFunction<Invocation, JsRawValue, RawArgList>,
    arity: Option<usize>,
    function_handle: Arc<JsFunctionHandle>,
}

struct Invocation {
    args: Vec<JsonValue>,
    sender: SharedSender,
    #[allow(dead_code)]
    focus: Option<Arc<JsonataFocus>>,
}

unsafe impl Send for JsFunctionCallable {}
unsafe impl Sync for JsFunctionCallable {}

impl JsFunctionCallable {
    fn new(env: &Env, func: JsFunction) -> napi::Result<Self> {
        let func_object = JsObject::from_raw(env.raw(), func.raw());
        let arity = match func_object.get_named_property::<JsUnknown>("length") {
            Ok(length_value) => match length_value.get_type()? {
                ValueType::Number => {
                    let numeric = length_value.coerce_to_number()?.get_int32()?;
                    if numeric >= 0 {
                        Some(numeric as usize)
                    } else {
                        None
                    }
                }
                _ => None,
            },
            Err(_) => None,
        };

        let function_handle = Arc::new(JsFunctionHandle::new(env.raw(), &func)?);

        let handle_for_callback = Arc::clone(&function_handle);
        let bridge = env.create_function_from_closure::<(), JsRawValue, _>("jsonataCallback", move |ctx| {
            if ctx.length() == 0 {
                return map_unknown(undefined(ctx.env)).map(JsRawValue);
            }
            let focus_arg: JsUnknown = if ctx.length() > 0 {
                ctx.get(0)?
            } else {
                undefined(ctx.env)?
            };
            let arg_count = ctx.length();
            let mut call_args: Vec<JsUnknown> = Vec::with_capacity(arg_count.saturating_sub(1));
            for index in 1..arg_count {
                call_args.push(ctx.get(index)?);
            }
            let focus_type_snapshot = focus_arg.get_type()?;
            let this_object = match focus_type_snapshot {
                ValueType::Undefined | ValueType::Null => None,
                _ => Some(focus_arg.coerce_to_object()?),
            };
            let js_function = handle_for_callback.to_js_function(ctx.env)?;
            match JsFunctionExt::call(&js_function, this_object.as_ref(), &call_args) {
                Ok(result) => Ok(JsRawValue(result.raw())),
                Err(err) => Err(err),
            }
        })?;

        let tsfn = bridge
            .build_threadsafe_function::<Invocation>()
            .callee_handled::<true>()
            .max_queue_size::<0>()
            .build_callback::<RawArgList, _>(move |ctx: ThreadsafeCallContext<Invocation>| {
                let Invocation { args, sender, focus } = ctx.value;
                let mut raw_args: Vec<sys::napi_value> = Vec::with_capacity(args.len() + 1);

                let focus_value = match focus.as_ref() {
                    Some(focus_data) => {
                        if let Some(handle) = &focus_data.handle {
                            if let Some(js_handle) =
                                handle.as_ref().downcast_ref::<JsThisHandle>()
                            {
                                match js_handle.to_js_unknown(&ctx.env) {
                                    Ok(value) => value,
                                    Err(err) => {
                                        sender.send(Err(JsonError::new(
                                            "RUST",
                                            format!("Failed to convert callback focus: {}", err),
                                        )));
                                        return Ok(RawArgList { values: Vec::new() });
                                    }
                                }
                            } else {
                                match json_value_to_js(&ctx.env, focus_data.input.clone()) {
                                    Ok(value) => value,
                                    Err(err) => {
                                        sender.send(Err(JsonError::new(
                                            "RUST",
                                            format!("Failed to convert callback focus: {}", err),
                                        )));
                                        return Ok(RawArgList { values: Vec::new() });
                                    }
                                }
                            }
                        } else {
                            match json_value_to_js(&ctx.env, focus_data.input.clone()) {
                                Ok(value) => value,
                                Err(err) => {
                                    sender.send(Err(JsonError::new(
                                        "RUST",
                                        format!("Failed to convert callback focus: {}", err),
                                    )));
                                    return Ok(RawArgList { values: Vec::new() });
                                }
                            }
                        }
                    }
                    None => undefined(&ctx.env)?,
                };
                raw_args.push(focus_value.raw());

                for value in args {
                    match json_value_to_js(&ctx.env, value) {
                        Ok(js_value) => raw_args.push(js_value.raw()),
                        Err(err) => {
                            sender.send(Err(JsonError::new(
                                "RUST",
                                format!("Failed to convert argument for callback: {}", err),
                            )));
                            return Ok(RawArgList { values: Vec::new() });
                        }
                    }
                }
                Ok(RawArgList { values: raw_args })
            })?;

        Ok(Self {
            tsfn,
            arity,
            function_handle,
        })
    }

    fn to_js_function(&self, env: &Env) -> napi::Result<JsFunction<'_>> {
        self.function_handle.to_js_function(env)
    }
}

impl JsonCallable for JsFunctionCallable {
    fn call(
        &self,
        ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, JsonCallResult> {
        eprintln!("[DEBUG] JsFunctionCallable::call with args: {:?}", args);
        let (sender, receiver) = oneshot::channel();
        let shared_sender = SharedSender::new(sender);
        let invocation = Invocation {
            args,
            sender: shared_sender.clone(),
            focus: ctx.focus(),
        };
        let callback_sender = shared_sender.clone();
        let status = self.tsfn.call_with_return_value(
            Ok(invocation),
            ThreadsafeFunctionCallMode::NonBlocking,
            move |result: napi::Result<JsRawValue>, env: Env| {
                eprintln!("[DEBUG] ThreadsafeFunction callback result: {:?}", result.is_ok());
                match result {
                    Ok(raw) => {
                        let value = unsafe { JsUnknown::from_raw_unchecked(env.raw(), raw.0) };
                        match value.is_promise() {
                            Ok(true) => {
                                eprintln!("[DEBUG] Value is a promise, attaching handlers");
                                if let Err(err) =
                                    attach_promise_handlers(&env, value, callback_sender.clone())
                                {
                                    eprintln!("[DEBUG] Error attaching promise handlers: {:?}", err);
                                    callback_sender.send(Err(napi_error_to_json("JS", err)));
                                }
                            }
                            Ok(false) => {
                                eprintln!("[DEBUG] Value is not a promise, converting directly");
                                let result = js_unknown_to_json_value(&env, value)
                                    .map_err(|err| napi_error_to_json("RUST", err));
                                eprintln!("[DEBUG] Conversion result: {:?}", result.is_ok());
                                callback_sender.send(result);
                            }
                            Err(err) => {
                                eprintln!("[DEBUG] Error checking if value is promise: {:?}", err);
                                callback_sender.send(Err(napi_error_to_json("JS", err)));
                            }
                        }
                    }
                    Err(err) => {
                        eprintln!("[DEBUG] ThreadsafeFunction callback error: {:?}", err);
                        callback_sender.send(Err(napi_error_to_json("JS", err)));
                    }
                };
                Ok(())
            },
        );

        if status != Status::Ok {
            shared_sender.send(Err(JsonError::new(
                "RUST",
                format!("Failed to schedule callback: {status:?}"),
            )));
        }

        Box::pin(async move {
            match receiver.await {
                Ok(result) => result,
                Err(_) => Err(JsonError::new(
                    "RUST",
                    "Callback channel closed before completion",
                )),
            }
        })
    }

    fn arity(&self) -> Option<usize> {
        self.arity
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}

fn attach_promise_handlers(
    env: &Env,
    promise: JsUnknown,
    sender: SharedSender,
) -> napi::Result<()> {
    let promise_object = promise.coerce_to_object()?;

    let resolve_sender = sender.clone();
    let resolve = env.create_function_from_closure::<(), sys::napi_value, _>("resolve", move |ctx| {
        let arg = if ctx.length() > 0 {
            ctx.get::<JsUnknown>(0)?
        } else {
            undefined(ctx.env)?
        };

        let result =
            js_unknown_to_json_value(ctx.env, arg).map_err(|err| napi_error_to_json("RUST", err));

        match result {
            Ok(value) => resolve_sender.send(Ok(value)),
            Err(err) => resolve_sender.send(Err(err)),
        }

        map_unknown(undefined(ctx.env))
    })?;

    let reject_sender = sender.clone();
    let reject = env.create_function_from_closure::<(), sys::napi_value, _>("reject", move |ctx| {
        let arg = if ctx.length() > 0 {
            ctx.get::<JsUnknown>(0)?
        } else {
            undefined(ctx.env)?
        };

        // Try to extract error information from JavaScript Error object
        let (code, message) = if let Ok(obj) = arg.coerce_to_object() {
            eprintln!("[DEBUG] Promise reject with object, checking properties");
            
            // Try to get error code
            let code = if obj.has_named_property("code")? {
                let code_prop: JsUnknown = obj.get_named_property("code")?;
                code_prop.coerce_to_string()
                    .and_then(|s| s.into_utf8())
                    .and_then(|utf| utf.as_str().map(|s| s.to_owned()))
                    .unwrap_or_else(|_| "JS".to_owned())
            } else {
                "JS".to_owned()
            };
            
            // Try to get error message
            let message = if obj.has_named_property("message")? {
                let msg_prop: JsUnknown = obj.get_named_property("message")?;
                msg_prop.coerce_to_string()
                    .and_then(|s| s.into_utf8())
                    .and_then(|utf| utf.as_str().map(|s| s.to_owned()))
                    .unwrap_or_else(|_| "Promise rejected with object".to_owned())
            } else {
                "Promise rejected with object".to_owned()
            };
            
            eprintln!("[DEBUG] Extracted error: code={}, message={}", code, message);
            (code, message)
        } else {
            // Fallback to string conversion
            let message = arg
                .coerce_to_string()
                .and_then(|value| value.into_utf8())
                .and_then(|utf| utf.as_str().map(|s| s.to_owned()))
                .unwrap_or_else(|_| "Promise rejected".to_owned());
            ("JS".to_owned(), message)
        };

        // Convert code to static str for JsonError::new  
        let static_code = if code == "D3138" {
            "D3138"
        } else if code == "D3139" {
            "D3139"
        } else if code.starts_with('D') && code.len() == 5 {
            // For other D-codes, we'll use JS as fallback since we can't convert String to &'static str
            "JS"
        } else {
            "JS"
        };
        reject_sender.send(Err(JsonError::new(static_code, message)));

        map_unknown(undefined(ctx.env))
    })?;

    let resolve_unknown = resolve.to_unknown();
    let reject_unknown = reject.to_unknown();

    let then_fn: JsFunction = promise_object.get_named_property("then")?;
    JsFunctionExt::call(&then_fn, Some(&promise_object), &[resolve_unknown, reject_unknown])?;

    Ok(())
}
