use futures::channel::oneshot;
use futures::future::BoxFuture;
use napi::bindgen_prelude::*;
use napi::sys;
use napi::threadsafe_function::{
    ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode,
};
use napi::{CallContext, Env, JsFunction, JsObject, JsUnknown, NapiRaw, NapiValue, Status, ValueType};
use napi_derive::napi;

use jsonata_rust::functions::{core as core_impl, math as math_impl, strings as strings_impl};
use jsonata_rust::types::{
    CallbackHandle, FunctionContext, JsonArray, JsonCallable, JsonError, JsonFunction,
    JsonObject, JsonValue, JsonataFocus,
};
use std::any::Any;
use std::sync::{Arc, Mutex};

fn option_number_to_js(env: &Env, value: Option<f64>) -> napi::Result<JsUnknown> {
    match value {
        Some(num) => env.create_double(num).map(|n| n.into_unknown()),
        None => env.get_undefined().map(|u| u.into_unknown()),
    }
}

fn get_number_arg(ctx: &CallContext, index: usize) -> napi::Result<Option<f64>> {
    if index >= ctx.length {
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

    let count = env.create_function_from_closure("count", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        json_value_to_js(ctx.env, math_impl::count_value(&value))
    })?;
    exports.set_named_property("count", count)?;

    let abs = env.create_function_from_closure("abs", |ctx| {
        let value = get_number_arg(&ctx, 0)?;
        option_number_to_js(ctx.env, math_impl::abs(value))
    })?;
    exports.set_named_property("abs", abs)?;

    let floor = env.create_function_from_closure("floor", |ctx| {
        let value = get_number_arg(&ctx, 0)?;
        option_number_to_js(ctx.env, math_impl::floor(value))
    })?;
    exports.set_named_property("floor", floor)?;

    let ceil = env.create_function_from_closure("ceil", |ctx| {
        let value = get_number_arg(&ctx, 0)?;
        option_number_to_js(ctx.env, math_impl::ceil(value))
    })?;
    exports.set_named_property("ceil", ceil)?;

    let round = env.create_function_from_closure("round", |ctx| {
        let value = get_number_arg(&ctx, 0)?;
        let precision = get_number_arg(&ctx, 1)?;
        option_number_to_js(ctx.env, math_impl::round(value, precision))
    })?;
    exports.set_named_property("round", round)?;

    let sqrt = env.create_function_from_closure("sqrt", |ctx| {
        let value = get_number_arg(&ctx, 0)?;
        match math_impl::sqrt(value) {
            Ok(result) => option_number_to_js(ctx.env, result),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("sqrt", sqrt)?;

    let power = env.create_function_from_closure("power", |ctx| {
        let base = get_number_arg(&ctx, 0)?;
        let exponent = get_number_arg(&ctx, 1)?;
        match math_impl::power(base, exponent) {
            Ok(result) => option_number_to_js(ctx.env, result),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("power", power)?;

    let number_fn = env.create_function_from_closure("number", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        match math_impl::number(&value) {
            Ok(result) => json_value_to_js(ctx.env, result),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("number", number_fn)?;

    let random = env.create_function_from_closure("random", |ctx| {
        let value = math_impl::random();
        ctx.env.create_double(value).map(|v| v.into_unknown())
    })?;
    exports.set_named_property("random", random)?;

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
        ValueType::Function => {
            let js_func = JsFunction::from_unknown(value)?;
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
        JsonValue::Function(function) => {
            let callable = function.as_callable();
            if let Some(js_callable) = callable.as_any().downcast_ref::<JsFunctionCallable>() {
                let js_func = js_callable.to_js_function(env)?;
                Ok(js_func.into_unknown())
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
        ValueType::Boolean => flag.coerce_to_bool()?.get_value(),
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

fn create_sequence_array(env: &Env, elements: Vec<JsUnknown>) -> napi::Result<JsUnknown> {
    let mut js_array: JsObject = env.create_array_with_length(elements.len())?;
    for (index, element) in elements.into_iter().enumerate() {
        js_array.set_element(index as u32, element)?;
    }
    let flag = env.get_boolean(true)?;
    js_array.set_named_property("sequence", flag)?;
    Ok(js_array.into_unknown())
}

fn lookup_js(env: &Env, value: JsUnknown, key: &str) -> napi::Result<Option<JsUnknown>> {
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
                                    aggregated.push(resolved_object.into_unknown());
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
    if index >= ctx.length {
        return Ok(JsonValue::Undefined);
    }
    let value: JsUnknown = ctx.get(index)?;
    js_unknown_to_json_value(ctx.env, value)
}

fn function_context_from_this(ctx: &CallContext) -> napi::Result<FunctionContext> {
    let this_value = ctx.this_unchecked::<JsUnknown>();
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

fn json_error_to_napi(err: JsonError) -> napi::Error {
    napi::Error::new(
        Status::GenericFailure,
        format!("{}: {}", err.code, err.message),
    )
}

fn napi_error_to_json(code: &'static str, err: napi::Error) -> JsonError {
    JsonError::new(code, err.to_string())
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
        let raw_value = unsafe { value.raw() };
        let status = unsafe { sys::napi_create_reference(env, raw_value, 1, &mut reference) };
        if status != sys::Status::napi_ok {
            return Err(Error::from_status(Status::from(status)));
        }
        Ok(Self { env, reference })
    }

    fn to_js_unknown(&self, env: &Env) -> napi::Result<JsUnknown> {
        let mut value = std::ptr::null_mut();
        let status = unsafe { sys::napi_get_reference_value(env.raw(), self.reference, &mut value) };
        if status != sys::Status::napi_ok {
            return Err(Error::from_status(Status::from(status)));
        }
        unsafe { JsUnknown::from_raw(env.raw(), value) }
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
        let raw_func = unsafe { func.raw() };
        let status = unsafe { sys::napi_create_reference(env, raw_func, 1, &mut reference) };
        if status != sys::Status::napi_ok {
            return Err(Error::from_status(Status::from(status)));
        }
        Ok(Self { env, reference })
    }

    fn to_js_function(&self, env: &Env) -> napi::Result<JsFunction> {
        let mut value = std::ptr::null_mut();
        let status = unsafe { sys::napi_get_reference_value(env.raw(), self.reference, &mut value) };
        if status != sys::Status::napi_ok {
            return Err(Error::from_status(Status::from(status)));
        }
        unsafe { JsFunction::from_raw(env.raw(), value) }
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
    let lookup = env.create_function_from_closure("lookup", |ctx| {
        if ctx.length < 2 {
            return ctx.env.get_undefined().map(|v| v.into_unknown());
        }
        let key: String = ctx.get(1)?;
        let input: JsUnknown = ctx.get(0)?;
        if let Some(value) = lookup_js(ctx.env, input, &key)? {
            Ok(value)
        } else {
            ctx.env.get_undefined().map(|v| v.into_unknown())
        }
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

    let zip_fn = env.create_function_from_closure("zip", |ctx| {
        let mut values: Vec<JsonValue> = Vec::with_capacity(ctx.length);
        for index in 0..ctx.length {
            values.push(arg_to_json_value(&ctx, index)?);
        }
        let result = core_impl::zip(&values);
        json_value_to_js(ctx.env, result)
    })?;
    exports.set_named_property("zip", zip_fn)?;

    let keys = env.create_function_from_closure("keys", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        let result = core_impl::keys(&value);
        json_value_to_js(ctx.env, result)
    })?;
    exports.set_named_property("keys", keys)?;

    let boolean_fn = env.create_function_from_closure("boolean", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        let result = core_impl::boolean(&value);
        json_value_to_js(ctx.env, result)
    })?;
    exports.set_named_property("boolean", boolean_fn)?;

    let not_fn = env.create_function_from_closure("not", |ctx| {
        let value = arg_to_json_value(&ctx, 0)?;
        let result = core_impl::not(&value);
        json_value_to_js(ctx.env, result)
    })?;
    exports.set_named_property("not", not_fn)?;

    let map_fn = env.create_function_from_closure("map", |ctx| {
        let array = arg_to_json_value(&ctx, 0)?;
        let func = arg_to_json_value(&ctx, 1)?;
        let focus = function_context_from_this(&ctx)?;
        ctx.env.execute_tokio_future(
            async move {
                core_impl::map(focus, array, func)
                    .await
                    .map_err(json_error_to_napi)
            },
            |env, result| json_value_to_js(env, result),
        )
    })?;
    exports.set_named_property("map", map_fn)?;

    let each_fn = env.create_function_from_closure("each", |ctx| {
        let input = arg_to_json_value(&ctx, 0)?;
        let func = arg_to_json_value(&ctx, 1)?;
        let focus = function_context_from_this(&ctx)?;
        ctx.env.execute_tokio_future(
            async move {
                core_impl::each(focus, input, func)
                    .await
                    .map_err(json_error_to_napi)
            },
            |env, result| json_value_to_js(env, result),
        )
    })?;
    exports.set_named_property("each", each_fn)?;

    let sort_fn = env.create_function_from_closure("sort", |ctx| {
        let array = arg_to_json_value(&ctx, 0)?;
        if ctx.length > 1 {
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
            Ok(result) => json_value_to_js(ctx.env, result),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("sort", sort_fn)?;

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
        "formatNumber",
        "formatBase",
        "filter",
        "single",
        "foldLeft",
        "sift",
        "spread",
        "merge",
        "reverse",
        "error",
        "assert",
        "type",
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

#[derive(Clone)]
struct JsFunctionCallable {
    tsfn: ThreadsafeFunction<Invocation>,
    env_raw: sys::napi_env,
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
        let func_object = unsafe { JsObject::from_raw_unchecked(env.raw(), func.raw()) };
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

        let original = func;
        let bridge = env.create_function_from_closure("jsonataCallback", move |ctx| {
            if ctx.length == 0 {
                return ctx.env.get_undefined().map(|v| v.into_unknown());
            }
            let focus_arg: JsUnknown = if ctx.length > 0 {
                ctx.get(0)?
            } else {
                ctx.env.get_undefined()?.into_unknown()
            };
            let mut call_args: Vec<JsUnknown> = Vec::with_capacity(ctx.length.saturating_sub(1));
            for index in 1..ctx.length {
                call_args.push(ctx.get(index)?);
            }
            let focus_type_snapshot = focus_arg.get_type()?;
            let this_object = match focus_type_snapshot {
                ValueType::Undefined | ValueType::Null => None,
                _ => Some(focus_arg.coerce_to_object()?),
            };
            match original.call(this_object.as_ref(), &call_args) {
                Ok(result) => Ok(JsUnknown::try_from(result)?),
                Err(err) => Err(err),
            }
        })?;

        let tsfn = env.create_threadsafe_function(
            &bridge,
            0,
            move |ctx: ThreadSafeCallContext<Invocation>| {
                let Invocation { args, sender, focus } = ctx.value;
                let mut converted: Vec<JsUnknown> = Vec::with_capacity(args.len() + 1);

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
                                            format!(
                                                "Failed to convert callback focus: {}",
                                                err
                                            ),
                                        )));
                                        return Ok(Vec::new());
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
                                        return Ok(Vec::new());
                                    }
                                }
                            }
                        } else {
                            match json_value_to_js(&ctx.env, focus_data.input.clone()) {
                                Ok(value) => value,
                                Err(err) => {
                                    sender.send(Err(JsonError::new(
                                        "RUST",
                                        format!(
                                            "Failed to convert callback focus: {}",
                                            err
                                        ),
                                    )));
                                    return Ok(Vec::new());
                                }
                            }
                        }
                    }
                    None => ctx.env.get_undefined()?.into_unknown(),
                };
                converted.push(focus_value);

                for value in args {
                    match json_value_to_js(&ctx.env, value) {
                        Ok(js_value) => converted.push(js_value),
                        Err(err) => {
                            sender.send(Err(JsonError::new(
                                "RUST",
                                format!("Failed to convert argument for callback: {}", err),
                            )));
                            return Ok(Vec::new());
                        }
                    }
                }
                Ok(converted)
            },
        )?;

        Ok(Self {
            tsfn,
            env_raw: env.raw(),
            arity,
            function_handle,
        })
    }

    fn env(&self) -> Env {
        // SAFETY: env_raw is valid for the lifetime of the addon
        unsafe { Env::from_raw(self.env_raw) }
    }

    fn to_js_function(&self, env: &Env) -> napi::Result<JsFunction> {
        self.function_handle.to_js_function(env)
    }
}

impl JsonCallable for JsFunctionCallable {
    fn call(
        &self,
        ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, JsonCallResult> {
        let (sender, receiver) = oneshot::channel();
        let shared_sender = SharedSender::new(sender);
        let invocation = Invocation {
            args,
            sender: shared_sender.clone(),
            focus: ctx.focus(),
        };
        let env = self.env();
        let callback_sender = shared_sender.clone();
        let status = self.tsfn.call_with_return_value::<JsUnknown, _>(
            Ok(invocation),
            ThreadsafeFunctionCallMode::NonBlocking,
            move |value: JsUnknown| {
                match value.is_promise() {
                    Ok(true) => {
                        if let Err(err) =
                            attach_promise_handlers(&env, value, callback_sender.clone())
                        {
                            callback_sender.send(Err(napi_error_to_json("JS", err)));
                        }
                    }
                    Ok(false) => {
                        let result = js_unknown_to_json_value(&env, value)
                            .map_err(|err| napi_error_to_json("RUST", err));
                        callback_sender.send(result);
                    }
                    Err(err) => {
                        callback_sender.send(Err(napi_error_to_json("JS", err)));
                    }
                }
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
    let resolve = env.create_function_from_closure("resolve", move |ctx: CallContext| {
        let arg = if ctx.length > 0 {
            ctx.get::<JsUnknown>(0)?
        } else {
            ctx.env.get_undefined()?.into_unknown()
        };

        let result =
            js_unknown_to_json_value(ctx.env, arg).map_err(|err| napi_error_to_json("RUST", err));

        match result {
            Ok(value) => resolve_sender.send(Ok(value)),
            Err(err) => resolve_sender.send(Err(err)),
        }

        ctx.env.get_undefined()
    })?;

    let reject_sender = sender.clone();
    let reject = env.create_function_from_closure("reject", move |ctx: CallContext| {
        let arg = if ctx.length > 0 {
            ctx.get::<JsUnknown>(0)?
        } else {
            ctx.env.get_undefined()?.into_unknown()
        };

        let message = arg
            .coerce_to_string()
            .and_then(|value| value.into_utf8())
            .and_then(|utf| utf.as_str().map(|s| s.to_owned()))
            .unwrap_or_else(|_| "Promise rejected".to_owned());

        reject_sender.send(Err(JsonError::new("JS", message)));

        ctx.env.get_undefined()
    })?;

    let resolve_unknown = resolve.into_unknown();
    let reject_unknown = reject.into_unknown();

    let then_fn: JsFunction = promise_object.get_named_property("then")?;
    then_fn.call(Some(&promise_object), &[resolve_unknown, reject_unknown])?;

    Ok(())
}
