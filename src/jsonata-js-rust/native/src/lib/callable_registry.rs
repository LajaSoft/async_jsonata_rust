use super::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, Weak};

const CALLABLE_ID_PROP: &str = "__rust_callable_id";
const CALLABLE_NONCE_PROP: &str = "__rust_callable_nonce";

struct CallableEntry {
    nonce: u64,
    callable: Weak<dyn JsonCallable>,
}

fn registry() -> &'static Mutex<HashMap<u64, CallableEntry>> {
    static REGISTRY: OnceLock<Mutex<HashMap<u64, CallableEntry>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

fn next_nonce() -> u64 {
    static NEXT_NONCE: AtomicU64 = AtomicU64::new(1001);
    NEXT_NONCE.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn register_callable(callable: Arc<dyn JsonCallable>) -> (u64, u64) {
    let mut guard = registry().lock().expect("callable registry lock poisoned");
    let mut stale_ids: Vec<u64> = Vec::new();
    for (id, entry) in guard.iter() {
        let Some(existing) = entry.callable.upgrade() else {
            stale_ids.push(*id);
            continue;
        };
        if Arc::ptr_eq(&existing, &callable) {
            return (*id, entry.nonce);
        }
    }
    for id in stale_ids {
        guard.remove(&id);
    }
    let id = next_id();
    let nonce = next_nonce();
    guard.insert(
        id,
        CallableEntry {
            nonce,
            callable: Arc::downgrade(&callable),
        },
    );
    (id, nonce)
}

pub(crate) fn lookup_callable(id: u64, nonce: u64) -> Option<Arc<dyn JsonCallable>> {
    let guard = registry().lock().ok()?;
    let entry = guard.get(&id)?;
    if entry.nonce != nonce {
        return None;
    }
    entry.callable.upgrade()
}

fn read_u64_metadata(object: &JsObject, name: &str) -> napi::Result<Option<u64>> {
    if !object.has_named_property(name)? {
        return Ok(None);
    }
    let value: JsUnknown = object.get_named_property(name)?;
    let parsed = match value.get_type()? {
        ValueType::Number => {
            let num = value.coerce_to_number()?.get_double()?;
            if !num.is_finite() || num < 0.0 {
                None
            } else {
                Some(num as u64)
            }
        }
        ValueType::String => {
            let text = value
                .coerce_to_string()?
                .into_utf8()?
                .as_str()?
                .to_owned();
            text.parse::<u64>().ok()
        }
        _ => None,
    };
    Ok(parsed)
}

pub(crate) fn resolve_registered_callable(
    object: &JsObject,
) -> napi::Result<Option<Arc<dyn JsonCallable>>> {
    let Some(id) = read_u64_metadata(object, CALLABLE_ID_PROP)? else {
        return Ok(None);
    };
    let Some(nonce) = read_u64_metadata(object, CALLABLE_NONCE_PROP)? else {
        return Ok(None);
    };
    Ok(lookup_callable(id, nonce))
}

pub(crate) fn attach_callable_metadata(
    env: &Env,
    object: &mut JsObject,
    callable: Arc<dyn JsonCallable>,
) -> napi::Result<()> {
    let (id, nonce) = register_callable(callable);
    object.set_named_property(CALLABLE_ID_PROP, id as f64)?;
    object.set_named_property(CALLABLE_NONCE_PROP, nonce as f64)?;
    let _ = env;
    Ok(())
}
