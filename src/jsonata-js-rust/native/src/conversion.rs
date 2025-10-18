use crate::js_unknown_to_json_value;
use napi::bindgen_prelude::*;
use napi::{Env, Status, ValueType};
use std::sync::Arc;
use jsonata_rust::{JsonataValue, JsonataArray, JsonataObject, NativeRef, NativeType};
use jsonata_rust::parser::ParserError;
use jsonata_rust::types::{JsonValue, JsonArray, JsonObject};
use serde_json::Value as SerdeValue;

// Структура для хранения napi reference
#[derive(Clone)]
pub struct NapiRef {
    env: napi::sys::napi_env,
    reference: napi::sys::napi_ref,
}

unsafe impl Send for NapiRef {}
unsafe impl Sync for NapiRef {}

impl NapiRef {
    pub fn new(env: &Env, value: &Unknown) -> napi::Result<Self> {
        let mut reference = std::ptr::null_mut();
        let raw_value = value.raw();
        let status = unsafe { 
            napi::sys::napi_create_reference(env.raw(), raw_value, 1, &mut reference) 
        };
        if status != napi::sys::Status::napi_ok {
            return Err(Error::from_status(Status::from(status)));
        }
        Ok(Self { 
            env: env.raw(), 
            reference 
        })
    }

    pub fn to_js_unknown<'env>(&self, env: &'env Env) -> napi::Result<Unknown<'env>> {
        let mut value = std::ptr::null_mut();
        let status = unsafe { 
            napi::sys::napi_get_reference_value(env.raw(), self.reference, &mut value) 
        };
        if status != napi::sys::Status::napi_ok {
            return Err(Error::from_status(Status::from(status)));
        }
        unsafe { Ok(Unknown::from_raw_unchecked(env.raw(), value)) }
    }
}

impl Drop for NapiRef {
    fn drop(&mut self) {
        unsafe {
            let _ = napi::sys::napi_delete_reference(self.env, self.reference);
        }
    }
}

// Конвертация JS -> JsonataValue
pub fn js_to_jsonata_value(env: &Env, value: Unknown) -> napi::Result<JsonataValue> {
    match value.get_type()? {
        ValueType::Undefined => Ok(JsonataValue::Undefined),
        ValueType::Null => Ok(JsonataValue::Null),
        ValueType::Boolean => Ok(JsonataValue::Bool(value.coerce_to_bool()?)),
        ValueType::Number => Ok(JsonataValue::Number(value.coerce_to_number()?.get_double()?)),
        ValueType::String => {
            let js_string = value.coerce_to_string()?;
            let utf8_string = js_string.into_utf8()?;
            let string_content = utf8_string.as_str()?.to_owned();
            Ok(JsonataValue::String(string_content))
        }
        ValueType::Function => {
            // Сохраняем JS функцию как NativeRef
            let napi_ref = NapiRef::new(env, &value)?;
            let handle = Arc::new(napi_ref) as Arc<dyn std::any::Any + Send + Sync>;
            Ok(JsonataValue::NativeRef(NativeRef {
                handle,
                value_type: NativeType::JsFunction,
            }))
        }
        ValueType::Object => {
            let object = value.coerce_to_object()?;
            if object.is_array()? {
                // Массив
                let length = object.get_array_length()?;
                let mut elements = Vec::with_capacity(length as usize);
                for index in 0..length {
                    let element: Unknown = object.get_element(index)?;
                    elements.push(js_to_jsonata_value(env, element)?);
                }
                
                // Проверяем специальные свойства JSONata
                let is_sequence = object.has_named_property("sequence")? && 
                    matches!(object.get_named_property::<Unknown>("sequence")?.get_type()?, ValueType::Boolean) &&
                    object.get_named_property::<Unknown>("sequence")?.coerce_to_bool()?;

                let outer_wrapper = object.has_named_property("outerWrapper")? && 
                    matches!(object.get_named_property::<Unknown>("outerWrapper")?.get_type()?, ValueType::Boolean) &&
                    object.get_named_property::<Unknown>("outerWrapper")?.coerce_to_bool()?;

                Ok(JsonataValue::Array(JsonataArray::new(elements, is_sequence, outer_wrapper)))
            } else {
                // Обычный объект - можем либо разобрать, либо сохранить как NativeRef
                // Для начала сохраним как NativeRef чтобы не потерять информацию
                let napi_ref = NapiRef::new(env, &value)?;
                let handle = Arc::new(napi_ref) as Arc<dyn std::any::Any + Send + Sync>;
                Ok(JsonataValue::NativeRef(NativeRef {
                    handle,
                    value_type: NativeType::JsObject,
                }))
            }
        }
        _ => {
            // Неизвестный тип - сохраняем как NativeRef
            let napi_ref = NapiRef::new(env, &value)?;
            let handle = Arc::new(napi_ref) as Arc<dyn std::any::Any + Send + Sync>;
            Ok(JsonataValue::NativeRef(NativeRef {
                handle,
                value_type: NativeType::JsOther,
            }))
        }
    }
}

// Конвертация JsonataValue -> JS
pub fn jsonata_value_to_js(env: &Env, value: JsonataValue) -> napi::Result<Unknown<'_>> {
    match value {
        JsonataValue::Undefined => ().into_unknown(env),
        JsonataValue::Null => {
            use napi::bindgen_prelude::Null;
            Null.into_unknown(env)
        },
        JsonataValue::Bool(b) => b.into_unknown(env),
        JsonataValue::Number(n) => env.create_double(n).and_then(|d| d.into_unknown(env)),
        JsonataValue::String(s) => env.create_string(&s).and_then(|s| s.into_unknown(env)),
        JsonataValue::Array(arr) => {
            let mut js_array = env.create_array(arr.elements.len() as u32)?;
            for (index, element) in arr.elements.into_iter().enumerate() {
                let js_element = jsonata_value_to_js(env, element)?;
                js_array.set(index as u32, js_element)?;
            }
            
            // Устанавливаем специальные свойства JSONata
            if arr.is_sequence {
                let mut js_array_obj = js_array.coerce_to_object()?;
                js_array_obj.set_named_property("sequence", true)?;
            }
            if arr.outer_wrapper {
                let mut js_array_obj = js_array.coerce_to_object()?;
                js_array_obj.set_named_property("outerWrapper", true)?;
            }
            
            js_array.into_unknown(env)
        }
        JsonataValue::Object(obj) => {
            let mut js_object = Object::new(env)?;
            for (key, val) in obj.0 {
                let js_val = jsonata_value_to_js(env, val)?;
                js_object.set_named_property(&key, js_val)?;
            }
            js_object.into_unknown(env)
        }
        JsonataValue::Function(_func) => {
            // TODO: Правильная конвертация функций - пока возвращаем undefined
            ().into_unknown(env)
        }
        JsonataValue::NativeRef(native_ref) => {
            let handle = native_ref.handle.clone();
            match Arc::downcast::<NapiRef>(handle) {
                Ok(napi_ref) => {
                    let js_value = napi_ref.to_js_unknown(env)?;
                    Ok(js_value)
                }
                Err(_) => ().into_unknown(env),
            }
        }
    }
}

// Конвертация для обратной совместимости: JsonValue -> JsonataValue  
pub fn json_value_to_jsonata_value(value: JsonValue) -> JsonataValue {
    match value {
        JsonValue::Undefined => JsonataValue::Undefined,
        JsonValue::Null => JsonataValue::Null,
        JsonValue::Bool(b) => JsonataValue::Bool(b),
        JsonValue::Number(n) => JsonataValue::Number(n),
        JsonValue::String(s) => JsonataValue::String(s),
        JsonValue::Array(arr) => {
            let elements = arr.elements.into_iter()
                .map(json_value_to_jsonata_value)
                .collect();
            JsonataValue::Array(JsonataArray::new(elements, arr.is_sequence, arr.outer_wrapper))
        }
        JsonValue::Object(obj) => {
            let props = obj.0.into_iter()
                .map(|(k, v)| (k, json_value_to_jsonata_value(v)))
                .collect();
            JsonataValue::Object(JsonataObject(props))
        }
        JsonValue::Function(_func) => {
            // TODO: Конвертация функций
            JsonataValue::Undefined
        }
    }
}

pub fn serde_value_to_js<'env>(env: &'env Env, value: &SerdeValue) -> napi::Result<Unknown<'env>> {
    match value {
        SerdeValue::Null => ().into_unknown(env),
        SerdeValue::Bool(b) => b.into_unknown(env),
        SerdeValue::Number(num) => {
            if let Some(i) = num.as_i64() {
                env.create_int64(i).and_then(|n| n.into_unknown(env))
            } else if let Some(u) = num.as_u64() {
                env
                    .create_double(u as f64)
                    .and_then(|n| n.into_unknown(env))
            } else if let Some(f) = num.as_f64() {
                env.create_double(f).and_then(|n| n.into_unknown(env))
            } else {
                env.create_string(&num.to_string())
                    .and_then(|s| s.into_unknown(env))
            }
        }
        SerdeValue::String(s) => env.create_string(s).and_then(|s| s.into_unknown(env)),
        SerdeValue::Array(items) => {
            let mut array = env.create_array(items.len() as u32)?;
            for (idx, item) in items.iter().enumerate() {
                let js_value = serde_value_to_js(env, item)?;
                array.set(idx as u32, js_value)?;
            }
            array.into_unknown(env)
        }
        SerdeValue::Object(map) => {
            let mut object = Object::new(env)?;
            for (key, val) in map {
                let js_value = serde_value_to_js(env, val)?;
                object.set_named_property(key, js_value)?;
            }
            object.into_unknown(env)
        }
    }
}

pub fn parser_error_to_js<'env>(env: &'env Env, err: &ParserError) -> napi::Result<Object<'env>> {
    let mut js_error = Object::new(env)?;
    js_error.set_named_property("code", env.create_string(&err.code)?)?;
    js_error.set_named_property("position", env.create_int64(err.position as i64)?)?;
    if let Some(token) = &err.token {
        let token_js = serde_value_to_js(env, token)?;
        js_error.set_named_property("token", token_js)?;
    }
    if let Some(value) = &err.value {
        let value_js = serde_value_to_js(env, value)?;
        js_error.set_named_property("value", value_js)?;
    }
    if let Some(remaining) = &err.remaining {
        let mut array = env.create_array(remaining.len() as u32)?;
        for (idx, item) in remaining.iter().enumerate() {
            let js_value = serde_value_to_js(env, item)?;
            array.set(idx as u32, js_value)?;
        }
        js_error.set_named_property("remaining", array)?;
    }
    Ok(js_error)
}

// Конвертация JsonataValue -> JsonValue для функций которые ещё не обновлены
pub fn jsonata_value_to_json_value(env: &Env, value: JsonataValue) -> napi::Result<JsonValue> {
    match value {
        JsonataValue::Undefined => Ok(JsonValue::Undefined),
        JsonataValue::Null => Ok(JsonValue::Null),
        JsonataValue::Bool(b) => Ok(JsonValue::Bool(b)),
        JsonataValue::Number(n) => Ok(JsonValue::Number(n)),
        JsonataValue::String(s) => Ok(JsonValue::String(s)),
        JsonataValue::Array(arr) => {
            let mut elements = Vec::with_capacity(arr.elements.len());
            for element in arr.elements {
                elements.push(jsonata_value_to_json_value(env, element)?);
            }
            Ok(JsonValue::Array(JsonArray::new(
                elements,
                arr.is_sequence,
                arr.outer_wrapper,
            )))
        }
        JsonataValue::Object(obj) => {
            let mut props = Vec::with_capacity(obj.0.len());
            for (k, v) in obj.0 {
                props.push((k, jsonata_value_to_json_value(env, v)?));
            }
            Ok(JsonValue::Object(JsonObject(props)))
        }
        JsonataValue::Function(_func) => {
            // TODO: Конвертация функций
            Ok(JsonValue::Undefined)
        }
        JsonataValue::NativeRef(native_ref) => {
            let handle = native_ref.handle.clone();
            match Arc::downcast::<NapiRef>(handle) {
                Ok(napi_ref) => {
                    let js_value = napi_ref.to_js_unknown(env)?;
                    js_unknown_to_json_value(env, js_value)
                }
                Err(_) => Ok(JsonValue::Undefined),
            }
        }
    }
}
