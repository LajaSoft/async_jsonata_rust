use super::*;

pub(crate) type JsonCallResult = std::result::Result<JsonValue, JsonError>;

pub(crate) struct JsThisHandle {
    env: sys::napi_env,
    reference: sys::napi_ref,
}

unsafe impl Send for JsThisHandle {}
unsafe impl Sync for JsThisHandle {}

impl JsThisHandle {
    pub(crate) fn new(env: sys::napi_env, value: &JsUnknown) -> napi::Result<Self> {
        let mut reference = std::ptr::null_mut();
        let raw_value = value.raw();
        let status = unsafe { sys::napi_create_reference(env, raw_value, 1, &mut reference) };
        if status != sys::Status::napi_ok {
            return Err(Error::from_status(Status::from(status)));
        }
        Ok(Self { env, reference })
    }

    pub(crate) fn to_js_unknown<'env>(&self, env: &'env Env) -> napi::Result<JsUnknown<'env>> {
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

pub(crate) struct JsFunctionHandle {
    env: sys::napi_env,
    reference: sys::napi_ref,
}

unsafe impl Send for JsFunctionHandle {}
unsafe impl Sync for JsFunctionHandle {}

impl JsFunctionHandle {
    pub(crate) fn new(env: sys::napi_env, func: &JsFunction) -> napi::Result<Self> {
        let mut reference = std::ptr::null_mut();
        let raw_func = func.raw();
        let status = unsafe { sys::napi_create_reference(env, raw_func, 1, &mut reference) };
        if status != sys::Status::napi_ok {
            return Err(Error::from_status(Status::from(status)));
        }
        Ok(Self { env, reference })
    }

    pub(crate) fn to_js_function<'env>(&self, env: &'env Env) -> napi::Result<JsFunction<'env>> {
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

pub(crate) struct SharedSender {
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
    pub(crate) fn new(sender: oneshot::Sender<JsonCallResult>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Some(sender))),
        }
    }

    pub(crate) fn take(&self) -> Option<oneshot::Sender<JsonCallResult>> {
        self.inner.lock().ok().and_then(|mut guard| guard.take())
    }

    pub(crate) fn send(&self, result: JsonCallResult) {
        if let Some(tx) = self.take() {
            let _ = tx.send(result);
        }
    }
}
