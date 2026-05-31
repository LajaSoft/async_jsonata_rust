use std::collections::HashMap;
use std::sync::Arc;

use crate::functions::core;
use crate::types::{JsonFunction, JsonValue};

use super::arg;
use super::callable::BuiltinCallable;

pub(super) fn register(registry: &mut HashMap<String, JsonFunction>) {
    registry.insert(
        "exists".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            Ok(core::exists(&arg(args, 0)))
        }))),
    );

    registry.insert(
        "boolean".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            Ok(core::boolean(&arg(args, 0)))
        }))),
    );

    registry.insert(
        "not".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            Ok(core::not(&arg(args, 0)))
        }))),
    );

    registry.insert(
        "type".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            Ok(core::type_of(&arg(args, 0)))
        }))),
    );

    registry.insert(
        "keys".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            Ok(core::keys(&arg(args, 0)))
        }))),
    );

    registry.insert(
        "zip".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(None, |args| Ok(core::zip(args))))),
    );

    registry.insert(
        "append".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(2), |args| {
            Ok(core::append(&arg(args, 0), &arg(args, 1)))
        }))),
    );

    registry.insert(
        "lookup".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(2), |args| {
            let input = arg(args, 0);
            let key_value = arg(args, 1);
            let key = match key_value {
                JsonValue::String(text) => text,
                JsonValue::Number(num) => {
                    if num.fract() == 0.0 {
                        (num as i64).to_string()
                    } else {
                        num.to_string()
                    }
                }
                JsonValue::Bool(flag) => flag.to_string(),
                JsonValue::Null => "null".to_owned(),
                JsonValue::Undefined => return Ok(JsonValue::Undefined),
                _ => return Ok(JsonValue::Undefined),
            };
            Ok(core::lookup(&input, &key))
        }))),
    );

    registry.insert(
        "distinct".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            core::distinct(&arg(args, 0))
        }))),
    );

    registry.insert(
        "reverse".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            core::reverse(&arg(args, 0))
        }))),
    );

    registry.insert(
        "merge".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            core::merge(&arg(args, 0))
        }))),
    );

    registry.insert(
        "spread".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            Ok(core::spread(&arg(args, 0)))
        }))),
    );

    registry.insert(
        "shuffle".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(1), |args| {
            core::shuffle(&arg(args, 0))
        }))),
    );

    registry.insert(
        "single".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(2), |ctx, args| {
            let array = arg(args, 0);
            let predicate = arg(args, 1);
            Box::pin(core::single(ctx, array, predicate))
        }))),
    );

    registry.insert(
        "map".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(2), |ctx, args| {
            let array = arg(args, 0);
            let func = arg(args, 1);
            Box::pin(core::map(ctx, array, func))
        }))),
    );

    registry.insert(
        "filter".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(2), |ctx, args| {
            let array = arg(args, 0);
            let func = arg(args, 1);
            Box::pin(core::filter(ctx, array, func))
        }))),
    );

    registry.insert(
        "each".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(2), |ctx, args| {
            let input = arg(args, 0);
            let func = arg(args, 1);
            Box::pin(core::each(ctx, input, func))
        }))),
    );

    registry.insert(
        "sift".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(2), |ctx, args| {
            let input = arg(args, 0);
            let func = arg(args, 1);
            Box::pin(core::sift(ctx, input, func))
        }))),
    );

    registry.insert(
        "foldLeft".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(3), |ctx, args| {
            let sequence = arg(args, 0);
            let func = arg(args, 1);
            let init = arg(args, 2);
            Box::pin(core::fold_left(ctx, sequence, func, init))
        }))),
    );

    registry.insert(
        "reduce".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(3), |ctx, args| {
            let sequence = arg(args, 0);
            let func = arg(args, 1);
            let init = arg(args, 2);
            Box::pin(core::fold_left(ctx, sequence, func, init))
        }))),
    );

    registry.insert(
        "sort".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(2), |ctx, args| {
            let array = arg(args, 0);
            let comparator = arg(args, 1);
            Box::pin(core::sort(ctx, array, comparator))
        }))),
    );
}
