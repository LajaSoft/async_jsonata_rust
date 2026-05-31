use std::collections::HashMap;
use std::sync::Arc;

use crate::functions::regex;
use crate::types::JsonFunction;

use super::arg;
use super::callable::BuiltinCallable;

pub(super) fn register(registry: &mut HashMap<String, JsonFunction>) {
    registry.insert(
        "contains".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(2), |ctx, args| {
            let input = arg(args, 0);
            let token = arg(args, 1);
            Box::pin(regex::contains_function(ctx, input, token))
        }))),
    );

    registry.insert(
        "match".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(3), |ctx, args| {
            let input = arg(args, 0);
            let matcher = arg(args, 1);
            let limit = arg(args, 2);
            Box::pin(regex::match_function(ctx, input, matcher, limit))
        }))),
    );

    registry.insert(
        "split".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(3), |ctx, args| {
            let input = arg(args, 0);
            let separator = arg(args, 1);
            let limit = arg(args, 2);
            Box::pin(regex::split_function(ctx, input, separator, limit))
        }))),
    );

    registry.insert(
        "replace".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::async_fn(Some(4), |ctx, args| {
            let input = arg(args, 0);
            let pattern = arg(args, 1);
            let replacement = arg(args, 2);
            let limit = arg(args, 3);
            Box::pin(regex::replace_function(
                ctx,
                input,
                pattern,
                replacement,
                limit,
            ))
        }))),
    );

    registry.insert(
        "join".to_string(),
        JsonFunction::new(Arc::new(BuiltinCallable::sync_fn(Some(0), |args| {
            // `$join` carries no context modifier in its signature
            // (`<a<s>s?:s>`), so the required first argument cannot be supplied
            // implicitly: calling `$join()` with no arguments is a signature
            // mismatch (T0410), distinct from `$join(missing)` which yields
            // undefined.
            if args.is_empty() {
                return Err(crate::types::JsonError::new(
                    "T0410",
                    "Argument 1 of function join does not match function signature",
                ));
            }
            let values = arg(args, 0);
            let separator = arg(args, 1);
            regex::join_function(values, separator)
        }))),
    );
}
