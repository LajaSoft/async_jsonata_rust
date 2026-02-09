use super::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

struct ThrownEntry {
    env: sys::napi_env,
    reference: sys::napi_ref,
}

unsafe impl Send for ThrownEntry {}
unsafe impl Sync for ThrownEntry {}

impl Drop for ThrownEntry {
    fn drop(&mut self) {
        unsafe {
            let _ = sys::napi_delete_reference(self.env, self.reference);
        }
    }
}

fn registry() -> &'static Mutex<HashMap<u64, ThrownEntry>> {
    static REGISTRY: OnceLock<Mutex<HashMap<u64, ThrownEntry>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn store_thrown_error(env: &Env, value: JsUnknown) -> napi::Result<u64> {
    let mut reference = std::ptr::null_mut();
    let status = unsafe { sys::napi_create_reference(env.raw(), value.raw(), 1, &mut reference) };
    if status != sys::Status::napi_ok {
        return Err(Error::from_status(Status::from(status)));
    }

    let id = next_id();
    let entry = ThrownEntry {
        env: env.raw(),
        reference,
    };
    registry()
        .lock()
        .map_err(|_| Error::new(Status::GenericFailure, "thrown error registry lock poisoned"))?
        .insert(id, entry);
    Ok(id)
}

pub(crate) fn store_thrown_error_raw(env: &Env, value: sys::napi_value) -> napi::Result<u64> {
    if value.is_null() {
        return Err(Error::new(
            Status::InvalidArg,
            "cannot store null thrown error value",
        ));
    }

    let mut reference = std::ptr::null_mut();
    let status = unsafe { sys::napi_create_reference(env.raw(), value, 1, &mut reference) };
    if status != sys::Status::napi_ok {
        return Err(Error::from_status(Status::from(status)));
    }

    let id = next_id();
    let entry = ThrownEntry {
        env: env.raw(),
        reference,
    };
    registry()
        .lock()
        .map_err(|_| Error::new(Status::GenericFailure, "thrown error registry lock poisoned"))?
        .insert(id, entry);
    Ok(id)
}

pub(crate) fn take_thrown_error<'env>(env: &'env Env, id: u64) -> napi::Result<Option<JsUnknown<'env>>> {
    let entry = registry()
        .lock()
        .map_err(|_| Error::new(Status::GenericFailure, "thrown error registry lock poisoned"))?
        .remove(&id);

    let Some(mut entry) = entry else {
        return Ok(None);
    };

    let mut value = std::ptr::null_mut();
    let status = unsafe { sys::napi_get_reference_value(env.raw(), entry.reference, &mut value) };
    if status != sys::Status::napi_ok {
        return Err(Error::from_status(Status::from(status)));
    }

    let mut ref_count = 0;
    let status = unsafe { sys::napi_reference_unref(env.raw(), entry.reference, &mut ref_count) };
    if status != sys::Status::napi_ok {
        return Err(Error::from_status(Status::from(status)));
    }
    if ref_count == 0 {
        let status = unsafe { sys::napi_delete_reference(env.raw(), entry.reference) };
        if status != sys::Status::napi_ok {
            return Err(Error::from_status(Status::from(status)));
        }
    }

    entry.reference = std::ptr::null_mut();
    entry.env = std::ptr::null_mut();

    Ok(Some(unsafe { JsUnknown::from_raw_unchecked(env.raw(), value) }))
}
