use std::collections::HashMap;
use std::sync::Arc;

use crate::functions::strings;
use crate::types::{JsonFunction, JsonValue};

use super::arg;
use super::callable::BuiltinCallable;

pub(super) fn register(registry: &mut HashMap<String, JsonFunction>) {
    registry.insert(
        "string".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            let input = arg(args, 0);
            let pretty = arg(args, 1);
            let is_pretty = matches!(pretty, JsonValue::Bool(true));
            strings::string(&input, is_pretty)
        }))),
    );

    registry.insert(
        "length".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            strings::length(&arg(args, 0))
        }))),
    );

    registry.insert(
        "substring".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(3), |args| {
            strings::substring(&arg(args, 0), &arg(args, 1), &arg(args, 2))
        }))),
    );

    registry.insert(
        "substringBefore".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(2), |args| {
            strings::substring_before(&arg(args, 0), &arg(args, 1))
        }))),
    );

    registry.insert(
        "substringAfter".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(2), |args| {
            strings::substring_after(&arg(args, 0), &arg(args, 1))
        }))),
    );

    registry.insert(
        "uppercase".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            strings::uppercase(&arg(args, 0))
        }))),
    );

    registry.insert(
        "lowercase".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            strings::lowercase(&arg(args, 0))
        }))),
    );

    registry.insert(
        "trim".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            strings::trim(&arg(args, 0))
        }))),
    );

    registry.insert(
        "pad".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(3), |args| {
            strings::pad(&arg(args, 0), &arg(args, 1), &arg(args, 2))
        }))),
    );

    registry.insert(
        "base64encode".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            strings::base64encode(&arg(args, 0))
        }))),
    );

    registry.insert(
        "base64decode".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            strings::base64decode(&arg(args, 0))
        }))),
    );

    registry.insert(
        "encodeUrlComponent".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            strings::encode_url_component(&arg(args, 0))
        }))),
    );

    registry.insert(
        "encodeUrl".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            strings::encode_url(&arg(args, 0))
        }))),
    );

    registry.insert(
        "decodeUrlComponent".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            strings::decode_url_component(&arg(args, 0))
        }))),
    );

    registry.insert(
        "decodeUrl".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            strings::decode_url(&arg(args, 0))
        }))),
    );

    registry.insert(
        "formatBase".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(2), |args| {
            strings::format_base(&arg(args, 0), &arg(args, 1))
        }))),
    );

    registry.insert(
        "formatNumber".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(3), |args| {
            strings::format_number(&arg(args, 0), &arg(args, 1), &arg(args, 2))
        }))),
    );
}
