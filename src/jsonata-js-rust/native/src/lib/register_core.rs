use super::*;

pub(crate) fn register_core(env: &Env, exports: &mut JsObject) -> napi::Result<()> {
    let lookup = env.create_function_from_closure::<(), sys::napi_value, _>("lookup", |ctx| {
        if ctx.length() < 2 {
            return map_unknown(undefined(ctx.env));
        }
        let key: String = ctx.get(1)?;
        let input: JsUnknown = ctx.get(0)?;
        if let Some(value) = lookup_js(ctx.env, input, &key)? {
            return Ok(value.raw());
        }
        map_unknown(undefined(ctx.env))
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
