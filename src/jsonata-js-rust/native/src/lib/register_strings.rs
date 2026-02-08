use super::*;

pub(crate) fn register_strings(env: &Env, exports: &mut JsObject) -> napi::Result<()> {
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

    let match_fn = env.create_function_from_closure::<(), sys::napi_value, _>("match", |ctx| {
        let input = arg_to_json_value(&ctx, 0)?;
        let matcher = arg_to_json_value(&ctx, 1)?;
        let limit = arg_to_json_value(&ctx, 2)?;
        let focus = function_context_from_this(&ctx)?;
        ctx.env
            .spawn_future_with_callback(
                async move {
                    match_function_impl(focus, input, matcher, limit)
                        .await
                        .map_err(json_error_to_napi)
                },
                |env, result| json_value_to_js(env, result),
            )
            .map(|promise| promise.raw())
    })?;
    exports.set_named_property("match", match_fn)?;

    let replace_fn = env.create_function_from_closure::<(), sys::napi_value, _>("replace", |ctx| {
        let input = arg_to_json_value(&ctx, 0)?;
        let pattern = arg_to_json_value(&ctx, 1)?;
        let replacement = arg_to_json_value(&ctx, 2)?;
        let limit = arg_to_json_value(&ctx, 3)?;
        let focus = function_context_from_this(&ctx)?;
        ctx.env
            .spawn_future_with_callback(
                async move {
                    replace_function_impl(focus, input, pattern, replacement, limit)
                        .await
                        .map_err(json_error_to_napi)
                },
                |env, result| json_value_to_js(env, result),
            )
            .map(|promise| promise.raw())
    })?;
    exports.set_named_property("replace", replace_fn)?;

    let contains_fn =
        env.create_function_from_closure::<(), sys::napi_value, _>("contains", |ctx| {
            let input = arg_to_json_value(&ctx, 0)?;
            let token = arg_to_json_value(&ctx, 1)?;
            let focus = function_context_from_this(&ctx)?;
            ctx.env
                .spawn_future_with_callback(
                    async move {
                        contains_function_impl(focus, input, token)
                            .await
                            .map_err(json_error_to_napi)
                    },
                    |env, result| json_value_to_js(env, result),
                )
                .map(|promise| promise.raw())
        })?;
    exports.set_named_property("contains", contains_fn)?;

    let split_fn = env.create_function_from_closure::<(), sys::napi_value, _>("split", |ctx| {
        let input = arg_to_json_value(&ctx, 0)?;
        let separator = arg_to_json_value(&ctx, 1)?;
        let limit = arg_to_json_value(&ctx, 2)?;
        let focus = function_context_from_this(&ctx)?;
        ctx.env
            .spawn_future_with_callback(
                async move {
                    split_function_impl(focus, input, separator, limit)
                        .await
                        .map_err(json_error_to_napi)
                },
                |env, result| json_value_to_js(env, result),
            )
            .map(|promise| promise.raw())
    })?;
    exports.set_named_property("split", split_fn)?;

    let join_fn = env.create_function_from_closure::<(), sys::napi_value, _>("join", |ctx| {
        let values = arg_to_json_value(&ctx, 0)?;
        let separator = arg_to_json_value(&ctx, 1)?;
        match join_function_impl(values, separator) {
            Ok(result) => map_unknown(json_value_to_js(ctx.env, result)),
            Err(err) => Err(json_error_to_napi(err)),
        }
    })?;
    exports.set_named_property("join", join_fn)?;

    Ok(())
}
