use super::*;

pub(crate) struct JsFunctionCallable {
    tsfn: ThreadsafeFunction<Invocation, JsRawValue, RawArgList>,
    arity: Option<usize>,
    function_handle: Arc<JsFunctionHandle>,
    jsonata_apply_adapter: bool,
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
    pub(crate) fn new(env: &Env, func: JsFunction) -> napi::Result<Self> {
        Self::new_with_mode(env, func, false)
    }

    pub(crate) fn new_with_mode(
        env: &Env,
        func: JsFunction,
        jsonata_apply_adapter: bool,
    ) -> napi::Result<Self> {
        let func_object = JsObject::from_raw(env.raw(), func.raw());
        let arity = if let Ok(arity_value) = func_object.get_named_property::<JsUnknown>("arity") {
            match arity_value.get_type()? {
                ValueType::Number => {
                    let numeric = arity_value.coerce_to_number()?.get_int32()?;
                    if numeric >= 0 {
                        Some(numeric as usize)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        } else {
            match func_object.get_named_property::<JsUnknown>("length") {
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
            }
        };

        let function_handle = Arc::new(JsFunctionHandle::new(env.raw(), &func)?);

        let handle_for_callback = Arc::clone(&function_handle);
        let bridge = env.create_function_from_closure::<(), JsRawValue, _>("jsonataCallback", move |ctx| {
            if ctx.length() == 0 {
                return map_unknown(undefined(ctx.env)).map(JsRawValue);
            }
            let arg_count = ctx.length();
            let first_arg: JsUnknown = ctx.get(0)?;
            let has_tsfn_prefix = matches!(first_arg.get_type()?, ValueType::Undefined | ValueType::Null);
            let base_index = if has_tsfn_prefix { 1 } else { 0 };

            let focus_arg: JsUnknown = if arg_count > base_index {
                ctx.get(base_index)?
            } else {
                undefined(ctx.env)?
            };

            let mut call_args: Vec<JsUnknown> = Vec::with_capacity(arg_count.saturating_sub(base_index + 1));
            for index in (base_index + 1)..arg_count {
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
                            if let Some(js_handle) = handle.as_ref().downcast_ref::<JsThisHandle>() {
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
            jsonata_apply_adapter,
        })
    }

    pub(crate) fn to_js_function<'env>(&self, env: &'env Env) -> napi::Result<JsFunction<'env>> {
        self.function_handle.to_js_function(env)
    }

    pub(crate) fn function_handle(&self) -> Arc<JsFunctionHandle> {
        Arc::clone(&self.function_handle)
    }

    pub(crate) fn arity_hint(&self) -> Option<usize> {
        self.arity
    }

    pub(crate) fn uses_jsonata_apply_adapter(&self) -> bool {
        self.jsonata_apply_adapter
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
            args: if self.jsonata_apply_adapter {
                let focus_arg = ctx
                    .focus()
                    .map(|focus| focus.input.clone())
                    .unwrap_or(JsonValue::Undefined);
                vec![
                    focus_arg,
                    JsonValue::Array(JsonArray::new(args, false, false)),
                ]
            } else {
                args
            },
            sender: shared_sender.clone(),
            focus: ctx.focus(),
        };
        let callback_sender = shared_sender.clone();
        let status = self.tsfn.call_with_return_value(
            Ok(invocation),
            ThreadsafeFunctionCallMode::NonBlocking,
            move |result: napi::Result<JsRawValue>, env: Env| {
                match result {
                    Ok(raw) => {
                        let value = unsafe { JsUnknown::from_raw_unchecked(env.raw(), raw.0) };
                        let value = match unwrap_iterable_result(&env, value) {
                            Ok(unwrapped) => unwrapped,
                            Err(err) => {
                                callback_sender.send(Err(napi_error_to_json("JS", err)));
                                return Ok(());
                            }
                        };
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
                    }
                    Err(err) => {
                        match preserve_foreign_exception(&env, err) {
                            Ok(foreign) => callback_sender.send(Err(foreign)),
                            Err(err) => callback_sender.send(Err(napi_error_to_json("JS", err))),
                        };
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

fn unwrap_iterable_result<'env>(_env: &'env Env, value: JsUnknown<'env>) -> napi::Result<JsUnknown<'env>> {
    if value.get_type()? != ValueType::Object {
        return Ok(value);
    }

    let object = value.coerce_to_object()?;
    // Matcher callback objects also expose `next`; do not consume them as iterators.
    if object.has_named_property("match")? && object.has_named_property("groups")? {
        return Ok(value);
    }
    if !object.has_named_property("next")? {
        return Ok(value);
    }
    let next_value: JsUnknown = object.get_named_property("next")?;
    if next_value.get_type()? != ValueType::Function {
        return Ok(value);
    }
    let next_fn = JsFunction::try_from_unknown(next_value)?;
    let iter_result = JsFunctionExt::call(&next_fn, Some(&object), &[])?;
    if iter_result.get_type()? != ValueType::Object {
        return Ok(iter_result);
    }
    let iter_object = iter_result.coerce_to_object()?;
    if !iter_object.has_named_property("value")? {
        return Ok(iter_result);
    }
    iter_object.get_named_property("value")
}

fn preserve_foreign_exception(
    env: &Env,
    err: napi::Error,
) -> std::result::Result<JsonError, napi::Error> {
    if let Some(marker) = parse_thrownjs_marker(&err.reason) {
        return Ok(JsonError::new("J9001", marker));
    }

    let pending_like = err.status == Status::PendingException || err.reason.contains("PendingException");
    if !pending_like {
        return Err(err);
    }

    let mut unknown: Option<JsUnknown> = None;
    let mut exception = std::ptr::null_mut();
    let status = unsafe { sys::napi_get_and_clear_last_exception(env.raw(), &mut exception) };
    if status == sys::Status::napi_ok && !exception.is_null() {
        if let Ok(id) = store_thrown_error_raw(env, exception) {
            return Ok(JsonError::new("J9001", format!("THROWN:{id}")));
        }
        unknown = Some(unsafe { JsUnknown::from_raw_unchecked(env.raw(), exception) });
    }
    if unknown.is_none() {
        let js_error = napi::JsError::from(err);
        unknown = Some(js_error.into_unknown(*env));
    }

    let Some(unknown) = unknown else {
        return Err(napi::Error::new(
            Status::GenericFailure,
            "Failed to preserve foreign JavaScript exception",
        ));
    };

    if let Some(marker) = extract_js_foreign_marker(&unknown) {
        return Ok(JsonError::new("J9001", marker));
    }

    let id = store_thrown_error(env, unknown).map_err(|store_err| {
        napi::Error::new(
            Status::GenericFailure,
            format!(
                "Failed to store foreign JavaScript exception for passthrough: status={:?}, reason={}",
                store_err.status,
                store_err.reason
            ),
        )
    })?;
    Ok(JsonError::new("J9001", format!("THROWN:{id}")))
}

fn extract_js_foreign_marker(value: &JsUnknown) -> Option<String> {
    if matches!(value.get_type().ok()?, ValueType::String) {
        let text = value
            .coerce_to_string()
            .ok()?
            .into_utf8()
            .ok()?
            .as_str()
            .ok()?
            .to_owned();
        return parse_thrownjs_marker(&text);
    }

    let object = value.coerce_to_object().ok()?;
    if !object.has_named_property("message").ok()? {
        return None;
    }

    if object.has_named_property("code").ok()? {
        let code_value: JsUnknown = object.get_named_property("code").ok()?;
        let code = code_value
            .coerce_to_string()
            .ok()?
            .into_utf8()
            .ok()?
            .as_str()
            .ok()?
            .to_owned();
        if !code.is_empty() && code != "J9001" {
            return None;
        }
    }

    let message_value: JsUnknown = object.get_named_property("message").ok()?;
    let message = message_value
        .coerce_to_string()
        .ok()?
        .into_utf8()
        .ok()?
        .as_str()
        .ok()?
        .to_owned();
    if !message.starts_with("THROWNJS:") {
        return None;
    }

    Some(message)
}

fn parse_thrownjs_marker(text: &str) -> Option<String> {
    let index = text.find("THROWNJS:")?;
    let suffix = &text[index..];
    let digits: String = suffix
        .chars()
        .skip("THROWNJS:".len())
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    Some(format!("THROWNJS:{digits}"))
}

fn attach_promise_handlers(env: &Env, promise: JsUnknown, sender: SharedSender) -> napi::Result<()> {
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

        let (code, message) = if let Ok(obj) = arg.coerce_to_object() {
            let code = if obj.has_named_property("code")? {
                let code_prop: JsUnknown = obj.get_named_property("code")?;
                code_prop
                    .coerce_to_string()
                    .and_then(|s| s.into_utf8())
                    .and_then(|utf| utf.as_str().map(|s| s.to_owned()))
                    .unwrap_or_else(|_| "JS".to_owned())
            } else {
                "JS".to_owned()
            };

            let message = if obj.has_named_property("message")? {
                let msg_prop: JsUnknown = obj.get_named_property("message")?;
                msg_prop
                    .coerce_to_string()
                    .and_then(|s| s.into_utf8())
                    .and_then(|utf| utf.as_str().map(|s| s.to_owned()))
                    .unwrap_or_else(|_| "Promise rejected with object".to_owned())
            } else {
                "Promise rejected with object".to_owned()
            };

            (code, message)
        } else {
            let message = arg
                .coerce_to_string()
                .and_then(|value| value.into_utf8())
                .and_then(|utf| utf.as_str().map(|s| s.to_owned()))
                .unwrap_or_else(|_| "Promise rejected".to_owned());
            ("JS".to_owned(), message)
        };

        let static_code = if code == "D3138" {
            "D3138"
        } else if code == "D3139" {
            "D3139"
        } else if code == "T2007" {
            "T2007"
        } else if code == "T2008" {
            "T2008"
        } else if code.starts_with('D') && code.len() == 5 {
            "JS"
        } else {
            "JS"
        };
        let full_message = if static_code == "JS" && code != "JS" {
            format!("{}: {}", code, message)
        } else {
            message
        };
        reject_sender.send(Err(JsonError::new(static_code, full_message)));

        map_unknown(undefined(ctx.env))
    })?;

    let resolve_unknown = resolve.to_unknown();
    let reject_unknown = reject.to_unknown();

    let then_fn: JsFunction = promise_object.get_named_property("then")?;
    JsFunctionExt::call(&then_fn, Some(&promise_object), &[resolve_unknown, reject_unknown])?;

    Ok(())
}
