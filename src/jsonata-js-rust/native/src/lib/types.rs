use super::*;
use std::ptr;

pub(crate) type JsUnknown<'env> = Unknown<'env>;
pub(crate) type JsObject<'env> = Object<'env>;
pub(crate) type JsFunction<'env> = Function<'env>;
pub(crate) type CallContext<'env> = FunctionCallContext<'env>;

pub(crate) fn undefined(env: &Env) -> napi::Result<JsUnknown<'_>> {
    ().into_unknown(env)
}

pub(crate) fn null(env: &Env) -> napi::Result<JsUnknown<'_>> {
    Null.into_unknown(env)
}

pub(crate) fn bool_to_unknown(env: &Env, value: bool) -> napi::Result<JsUnknown<'_>> {
    value.into_unknown(env)
}

pub(crate) fn map_unknown(result: napi::Result<JsUnknown>) -> napi::Result<sys::napi_value> {
    result.map(|value| value.raw())
}

#[derive(Clone, Copy)]
pub(crate) struct JsRawValue(pub(crate) sys::napi_value);

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

pub(crate) struct RawArgList {
    pub(crate) values: Vec<sys::napi_value>,
}

impl JsValuesTupleIntoVec for RawArgList {
    fn into_vec(mut self, _env: sys::napi_env) -> napi::Result<Vec<sys::napi_value>> {
        Ok(std::mem::take(&mut self.values))
    }
}

pub(crate) trait JsFunctionExt<'env> {
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
        let this_value = match this {
            Some(this_obj) => this_obj.raw(),
            None => {
                let mut undefined = ptr::null_mut();
                check_status!(unsafe { sys::napi_get_undefined(env, &mut undefined) })?;
                undefined
            }
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

pub(crate) fn option_number_to_js(env: &Env, value: Option<f64>) -> napi::Result<JsUnknown<'_>> {
    match value {
        Some(num) => env.create_double(num).and_then(|n| n.into_unknown(env)),
        None => undefined(env),
    }
}

pub(crate) fn get_number_arg(ctx: &CallContext, index: usize) -> napi::Result<Option<f64>> {
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

pub(crate) fn extract_numeric_args(ctx: &CallContext) -> napi::Result<Option<Vec<f64>>> {
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
            if !object.is_array()? {
                return Err(Error::new(
                    Status::InvalidArg,
                    "Expected an array of numbers for math helper",
                ));
            }
            let values: Vec<f64> = ctx.get(0)?;
            Ok(Some(values))
        }
        _ => Err(Error::new(
            Status::InvalidArg,
            "Unsupported argument type for math helper",
        )),
    }
}

pub(crate) fn property_flag_is_truthy(object: &JsObject, name: &str) -> napi::Result<bool> {
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

pub(crate) fn is_jsonata_function_object(object: &JsObject) -> napi::Result<bool> {
    if property_flag_is_truthy(object, "_jsonata_function")? {
        return Ok(true);
    }
    if property_flag_is_truthy(object, "_jsonata_lambda")? {
        return Ok(true);
    }
    Ok(false)
}

pub(crate) fn create_sequence_array<'env>(
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

pub(crate) fn lookup_js<'env>(
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
                if !object.has_named_property(key)? {
                    return Ok(None);
                }

                let property: JsUnknown = object.get_named_property(key)?;
                if matches!(property.get_type()?, ValueType::Undefined) {
                    return Ok(None);
                }
                Ok(Some(property))
            }
        }
        _ => Ok(None),
    }
}
