use super::*;

pub(crate) fn register_math(env: &Env, exports: &mut JsObject) -> napi::Result<()> {
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
