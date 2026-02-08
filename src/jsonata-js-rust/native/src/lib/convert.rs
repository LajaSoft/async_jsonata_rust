use super::*;

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
            let mut jsonata_apply_adapter = false;

            if let Ok(js_obj) = js_func.coerce_to_object() {
                if js_obj.has_named_property("__jsonata_apply_adapter")? {
                    let marker: JsUnknown = js_obj.get_named_property("__jsonata_apply_adapter")?;
                    jsonata_apply_adapter = matches!(marker.get_type()?, ValueType::Boolean)
                        && marker.coerce_to_bool()?;
                }
                if let Some((source, flags)) = extract_regex_meta_from_function_object(&js_obj)? {
                    return Ok(JsonValue::Object(JsonObject(vec![
                        ("__jsonata_regex_source".to_owned(), JsonValue::String(source)),
                        ("__jsonata_regex_flags".to_owned(), JsonValue::String(flags)),
                    ])));
                }
            }

            if let Ok(js_obj) = js_func.coerce_to_object() {
                if let Ok(builtin_name_value) = js_obj.get_named_property::<JsUnknown>("_rustBuiltin") {
                    if let Ok(name_str) = builtin_name_value.coerce_to_string() {
                        if let Ok(utf8_str) = name_str.into_utf8() {
                            if let Ok(name) = utf8_str.as_str() {
                                if !name.is_empty() && name != "undefined" {
                                    if let Some(builtin) = registry::lookup_builtin(name) {
                                        return Ok(JsonValue::Function(builtin));
                                    }
                                }
                            }
                        }
                    }
                }

                if let Ok(to_string_fn) = js_obj.get_named_property::<JsFunction>("toString") {
                    if let Ok(to_string_result) = JsFunctionExt::call(&to_string_fn, Some(&js_obj), &[]) {
                        if let Ok(string_result) = to_string_result.coerce_to_string() {
                            if let Ok(utf8_result) = string_result.into_utf8() {
                                if let Ok(string_content) = utf8_result.as_str() {
                                    if string_content.contains("[native code]") {
                                        if let Some(start) = string_content.find("function ") {
                                            let after_function = &string_content[start + 9..];
                                            if let Some(end) = after_function.find('(') {
                                                let func_name = after_function[..end].trim();
                                                if !func_name.is_empty() {
                                                    if let Some(builtin) = registry::lookup_builtin(func_name) {
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

                if let Ok(impl_value) = js_obj.get_named_property::<JsUnknown>("_rustImpl") {
                    if impl_value.get_type()? == ValueType::Function {
                        let impl_result = js_unknown_to_json_value(env, impl_value)?;
                        if let JsonValue::Function(_) = impl_result {
                            return Ok(impl_result);
                        }
                    }
                }

                if let Ok(name_value) = js_obj.get_named_property::<JsUnknown>("name") {
                    if let Ok(name_str) = name_value.coerce_to_string() {
                        if let Ok(utf8_str) = name_str.into_utf8() {
                            if let Ok(name) = utf8_str.as_str() {
                                if let Some(builtin) = registry::lookup_builtin(name) {
                                    return Ok(JsonValue::Function(builtin));
                                }
                            }
                        }
                    }
                }
            }

            let callable = JsFunctionCallable::new_with_mode(env, js_func, jsonata_apply_adapter)?;
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
                            let mut apply_object = apply_value.coerce_to_object()?;
                            apply_object.set_named_property("__jsonata_apply_adapter", true)?;
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

fn extract_regex_meta_from_function_object(
    object: &JsObject,
) -> napi::Result<Option<(String, String)>> {
    if object.has_named_property("__jsonata_regex")? {
        let regex_meta: JsUnknown = object.get_named_property("__jsonata_regex")?;
        if regex_meta.get_type()? == ValueType::Object {
            let regex_meta_obj = regex_meta.coerce_to_object()?;
            let source: String = regex_meta_obj
                .get_named_property::<JsUnknown>("source")?
                .coerce_to_string()?
                .into_utf8()?
                .as_str()?
                .to_owned();
            let flags: String = regex_meta_obj
                .get_named_property::<JsUnknown>("flags")?
                .coerce_to_string()?
                .into_utf8()?
                .as_str()?
                .to_owned();
            return Ok(Some((source, flags)));
        }
    }

    if object.has_named_property("implementation")? {
        let implementation: JsUnknown = object.get_named_property("implementation")?;
        if implementation.get_type()? == ValueType::Function {
            let implementation_object = implementation.coerce_to_object()?;
            if let Some(meta) = extract_regex_meta_from_function_object(&implementation_object)? {
                return Ok(Some(meta));
            }
        }
    }

    Ok(None)
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
                return js_func.into_unknown(env);
            }
            Err(Error::new(
                Status::InvalidArg,
                "Cannot convert function value to JavaScript",
            ))
        }
    }
}

pub(crate) fn arg_to_json_value(ctx: &CallContext, index: usize) -> napi::Result<JsonValue> {
    if index >= ctx.length() {
        return Ok(JsonValue::Undefined);
    }
    let value: JsUnknown = ctx.get(index)?;
    js_unknown_to_json_value(ctx.env, value)
}

pub(crate) fn function_context_from_this(ctx: &CallContext) -> napi::Result<FunctionContext> {
    let this_value = ctx.this::<JsUnknown>()?;
    match this_value.get_type()? {
        ValueType::Undefined | ValueType::Null => Ok(FunctionContext::empty()),
        _ => {
            let handle = Arc::new(JsThisHandle::new(ctx.env.raw(), &this_value)?);
            let focus_value = match handle.to_js_unknown(ctx.env) {
                Ok(js_this) => {
                    js_unknown_to_json_value(ctx.env, js_this).unwrap_or(JsonValue::Undefined)
                }
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
    napi::Error::new(
        Status::GenericFailure,
        format!("{}: {}", err.code, err.message),
    )
}

pub(crate) fn napi_error_to_json(code: &'static str, err: napi::Error) -> JsonError {
    let message = err.to_string();

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
        return JsonError::new(code, format!("{} (status: {:?})", reason, err.status));
    }

    JsonError::new(code, message)
}
