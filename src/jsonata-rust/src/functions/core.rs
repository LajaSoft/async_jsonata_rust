use std::cmp::Ordering;
use std::collections::HashSet;

use rand::Rng;

use crate::functions::strings;
use crate::types::{
    FunctionContext, JsonArray, JsonError, JsonFunction, JsonObject, JsonValue, JsonataArray,
    JsonataValue,
};

fn clone_array_elements(array: &JsonArray) -> Vec<JsonValue> {
    array.elements.clone()
}

pub fn lookup(input: &JsonValue, key: &str) -> JsonValue {
    match input {
        JsonValue::Undefined => JsonValue::Undefined,
        JsonValue::Array(array) => {
            let mut results: Vec<JsonValue> = Vec::new();
            for item in &array.elements {
                let resolved = lookup(item, key);
                match resolved {
                    JsonValue::Undefined => {}
                    JsonValue::Array(seq) => {
                        for element in seq.elements {
                            results.push(element);
                        }
                    }
                    value => results.push(value),
                }
            }
            if results.is_empty() {
                JsonValue::Undefined
            } else {
                JsonValue::Array(JsonArray::new(results, true, false))
            }
        }
        JsonValue::Object(JsonObject(props)) => {
            for (prop_key, value) in props {
                if prop_key == key {
                    return value.clone();
                }
            }
            JsonValue::Undefined
        }
        _ => JsonValue::Undefined,
    }
}

pub fn append(left: &JsonValue, right: &JsonValue) -> JsonValue {
    if left.is_undefined() {
        return right.clone();
    }
    if right.is_undefined() {
        return left.clone();
    }

    let mut combined: Vec<JsonValue> = match left {
        JsonValue::Array(array) => array.elements.clone(),
        value => vec![value.clone()],
    };

    match right {
        JsonValue::Array(array) => combined.extend(array.elements.clone()),
        value => combined.push(value.clone()),
    }

    JsonValue::Array(JsonArray::new(combined, false, false))
}

pub fn append_jsonata(left: &JsonataValue, right: &JsonataValue) -> JsonataValue {
    if left.is_undefined() {
        return right.clone();
    }
    if right.is_undefined() {
        return left.clone();
    }

    let mut combined: Vec<JsonataValue> = match left {
        JsonataValue::Array(array) => array.elements.clone(),
        value => vec![value.clone()],
    };

    match right {
        JsonataValue::Array(array) => combined.extend(array.elements.clone()),
        value => combined.push(value.clone()),
    }

    JsonataValue::Array(JsonataArray::new(combined, false, false))
}

fn coerce_zip_sequence(value: &JsonValue) -> Vec<JsonValue> {
    match value {
        JsonValue::Undefined => Vec::new(),
        JsonValue::Array(array) => {
            if array.outer_wrapper && !array.is_sequence {
                vec![JsonValue::Array(JsonArray::new(
                    array.elements.clone(),
                    array.is_sequence,
                    array.outer_wrapper,
                ))]
            } else {
                clone_array_elements(array)
            }
        }
        other => vec![other.clone()],
    }
}

pub fn zip(args: &[JsonValue]) -> JsonValue {
    if args.is_empty() {
        return JsonValue::Array(JsonArray::new(Vec::new(), false, false));
    }

    let mut sequences: Vec<Vec<JsonValue>> = Vec::with_capacity(args.len());
    let mut min_len = usize::MAX;

    for arg in args {
        let entries = coerce_zip_sequence(arg);
        min_len = min_len.min(entries.len());
        sequences.push(entries);
    }

    if min_len == usize::MAX {
        min_len = 0;
    }

    let mut zipped: Vec<JsonValue> = Vec::with_capacity(min_len);
    for index in 0..min_len {
        let mut tuple: Vec<JsonValue> = Vec::with_capacity(sequences.len());
        for sequence in sequences.iter() {
            tuple.push(sequence[index].clone());
        }
        zipped.push(JsonValue::Array(JsonArray::new(tuple, false, false)));
    }

    JsonValue::Array(JsonArray::new(zipped, false, false))
}

pub fn exists(value: &JsonValue) -> JsonValue {
    JsonValue::Bool(!value.is_undefined())
}

pub fn type_of(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Undefined => JsonValue::Undefined,
        JsonValue::Null => JsonValue::String("null".to_owned()),
        JsonValue::Bool(_) => JsonValue::String("boolean".to_owned()),
        JsonValue::Number(_) => JsonValue::String("number".to_owned()),
        JsonValue::String(_) => JsonValue::String("string".to_owned()),
        JsonValue::Array(_) => JsonValue::String("array".to_owned()),
        JsonValue::Object(_) => JsonValue::String("object".to_owned()),
        JsonValue::Function(_) => JsonValue::String("function".to_owned()),
    }
}

pub fn keys(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Undefined => JsonValue::Array(JsonArray::empty_sequence()),
        JsonValue::Array(array) => {
            let mut seen = HashSet::new();
            let mut ordered: Vec<JsonValue> = Vec::new();
            for item in &array.elements {
                if let JsonValue::Array(seq) = keys(item) {
                    for element in seq.elements {
                        if let JsonValue::String(key) = element {
                            if seen.insert(key.clone()) {
                                ordered.push(JsonValue::String(key));
                            }
                        }
                    }
                }
            }
            JsonValue::Array(JsonArray::new(ordered, true, false))
        }
        JsonValue::Object(JsonObject(props)) => {
            let mut ordered = Vec::with_capacity(props.len());
            for (name, _) in props {
                ordered.push(JsonValue::String(name.clone()));
            }
            JsonValue::Array(JsonArray::new(ordered, true, false))
        }
        _ => JsonValue::Array(JsonArray::empty_sequence()),
    }
}

fn boolean_internal(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Undefined => JsonValue::Undefined,
        JsonValue::Null => JsonValue::Bool(false),
        JsonValue::Bool(flag) => JsonValue::Bool(*flag),
        JsonValue::Number(num) => JsonValue::Bool(*num != 0.0),
        JsonValue::String(text) => JsonValue::Bool(!text.is_empty()),
        JsonValue::Array(array) => match array.elements.len() {
            0 => JsonValue::Bool(false),
            1 => boolean_internal(&array.elements[0]),
            _ => {
                let mut truthy = false;
                for element in &array.elements {
                    if matches!(boolean_internal(element), JsonValue::Bool(true)) {
                        truthy = true;
                        break;
                    }
                }
                JsonValue::Bool(truthy)
            }
        },
        JsonValue::Object(JsonObject(props)) => JsonValue::Bool(!props.is_empty()),
        JsonValue::Function(_) => JsonValue::Bool(false),
    }
}

pub fn boolean(value: &JsonValue) -> JsonValue {
    boolean_internal(value)
}

pub fn not(value: &JsonValue) -> JsonValue {
    match boolean_internal(value) {
        JsonValue::Undefined => JsonValue::Undefined,
        JsonValue::Bool(flag) => JsonValue::Bool(!flag),
        other => other,
    }
}

fn ensure_homogeneous_sort_elements(elements: &[JsonValue]) -> Result<SortDomain, JsonError> {
    let mut has_numbers = false;
    let mut has_strings = false;

    for element in elements {
        match element {
            JsonValue::Number(_) => has_numbers = true,
            JsonValue::String(_) => has_strings = true,
            JsonValue::Undefined => {
                return Err(JsonError::new(
                    "D3070",
                    "Sort expects an array of numbers or strings",
                ))
            }
            _ => {
                return Err(JsonError::new(
                    "D3070",
                    "Sort expects an array of numbers or strings",
                ))
            }
        }
        if has_numbers && has_strings {
            return Err(JsonError::new(
                "D3070",
                "Sort expects an array of numbers or strings",
            ));
        }
    }

    if has_numbers {
        Ok(SortDomain::Numbers)
    } else if has_strings {
        Ok(SortDomain::Strings)
    } else {
        Ok(SortDomain::Empty)
    }
}

enum SortDomain {
    Numbers,
    Strings,
    Empty,
}

pub fn sort_default(array: &JsonValue) -> Result<JsonValue, JsonError> {
    let input_array = match array {
        JsonValue::Undefined => return Ok(JsonValue::Undefined),
        JsonValue::Array(arr) => arr,
        _ => {
            return Err(JsonError::new(
                "D3070",
                "Sort expects an array as the first argument",
            ))
        }
    };

    if input_array.elements.len() <= 1 {
        return Ok(JsonValue::Array(JsonArray::new(
            input_array.elements.clone(),
            input_array.is_sequence,
            input_array.outer_wrapper,
        )));
    }

    let mut elements = input_array.elements.clone();
    match ensure_homogeneous_sort_elements(&elements)? {
        SortDomain::Numbers => {
            elements.sort_by(|left, right| match (left, right) {
                (JsonValue::Number(a), JsonValue::Number(b)) => {
                    a.partial_cmp(b).unwrap_or(Ordering::Equal)
                }
                _ => Ordering::Equal,
            });
        }
        SortDomain::Strings => {
            elements.sort_by(|left, right| match (left, right) {
                (JsonValue::String(a), JsonValue::String(b)) => a.cmp(b),
                _ => Ordering::Equal,
            });
        }
        SortDomain::Empty => {}
    }

    Ok(JsonValue::Array(JsonArray::new(
        elements,
        input_array.is_sequence,
        input_array.outer_wrapper,
    )))
}

pub async fn sort(
    ctx: FunctionContext,
    array: JsonValue,
    comparator: JsonValue,
) -> Result<JsonValue, JsonError> {
    let input_array = match array {
        JsonValue::Undefined => return Ok(JsonValue::Undefined),
        JsonValue::Array(arr) => arr,
        _ => {
            return Err(JsonError::new(
                "D3070",
                "Sort expects an array as the first argument",
            ))
        }
    };

    if input_array.elements.len() <= 1 {
        return Ok(JsonValue::Array(JsonArray::new(
            input_array.elements.clone(),
            input_array.is_sequence,
            input_array.outer_wrapper,
        )));
    }

    if matches!(comparator, JsonValue::Undefined | JsonValue::Null) {
        return sort_default(&JsonValue::Array(input_array));
    }

    let callable = match comparator {
        JsonValue::Function(func) => func,
        _ => {
            return Err(JsonError::new(
                "D3070",
                "Comparator must be a function",
            ))
        }
    };

    let mut sorted: Vec<JsonValue> = Vec::with_capacity(input_array.elements.len());

    for item in input_array.elements {
        let mut insert_index = sorted.len();
        while insert_index > 0 {
            let args = build_hof_args(
                &callable,
                sorted[insert_index - 1].clone(),
                Some(item.clone()),
                None,
            );
            let decision = callable.call(ctx.clone(), args).await?;
            let should_swap = matches!(boolean(&decision), JsonValue::Bool(true));
            if !should_swap {
                break;
            }
            insert_index -= 1;
        }
        sorted.insert(insert_index, item);
    }

    Ok(JsonValue::Array(JsonArray::new(
        sorted,
        input_array.is_sequence,
        input_array.outer_wrapper,
    )))
}

fn build_hof_args(
    callable: &JsonFunction,
    first: JsonValue,
    second: Option<JsonValue>,
    third: Option<JsonValue>,
) -> Vec<JsonValue> {
    let arity = callable.arity().unwrap_or(3);

    let mut args = Vec::with_capacity(
        1 + second.as_ref().map(|_| 1).unwrap_or_default()
            + third.as_ref().map(|_| 1).unwrap_or_default(),
    );

    args.push(first);

    if arity >= 2 {
        if let Some(value) = second {
            args.push(value);
        }
    }

    if arity >= 3 {
        if let Some(value) = third {
            args.push(value);
        }
    }

    args
}

pub async fn map(
    ctx: FunctionContext,
    array: JsonValue,
    func: JsonValue,
) -> Result<JsonValue, JsonError> {
    let (arr, container) = match array {
        JsonValue::Undefined => return Ok(JsonValue::Undefined),
        JsonValue::Array(arr) => {
            let container = JsonValue::Array(arr.clone());
            (arr, container)
        }
        _ => {
            return Err(JsonError::new(
                "D3050",
                "$map() expects the first argument to be an array",
            ))
        }
    };

    let callable = match func {
        JsonValue::Function(func) => func,
        _ => {
            return Err(JsonError::new(
                "D3050",
                "$map() expects the second argument to be a function",
            ))
        }
    };

    let mut results = Vec::with_capacity(arr.elements.len());

    for (index, element) in arr.elements.iter().enumerate() {
        let args = build_hof_args(
            &callable,
            element.clone(),
            Some(JsonValue::Number(index as f64)),
            Some(container.clone()),
        );

        let value = callable.call(ctx.clone(), args).await?;
        if !value.is_undefined() {
            results.push(value);
        }
    }

    Ok(JsonValue::Array(JsonArray::new(results, true, false)))
}

pub async fn filter(
    ctx: FunctionContext,
    array: JsonValue,
    func: JsonValue,
) -> Result<JsonValue, JsonError> {
    let arr = match array {
        JsonValue::Undefined => return Ok(JsonValue::Undefined),
        JsonValue::Array(arr) => arr,
        _ => {
            return Err(JsonError::new(
                "D3050",
                "$filter() expects the first argument to be an array",
            ))
        }
    };

    let callable = match func {
        JsonValue::Function(func) => func,
        _ => {
            return Err(JsonError::new(
                "D3050",
                "$filter() expects the second argument to be a function",
            ))
        }
    };

    let container = JsonValue::Array(arr.clone());
    let mut results: Vec<JsonValue> = Vec::new();

    for (index, entry) in arr.elements.iter().enumerate() {
        let args = build_hof_args(
            &callable,
            entry.clone(),
            Some(JsonValue::Number(index as f64)),
            Some(container.clone()),
        );
        let predicate = callable.call(ctx.clone(), args).await?;
        if matches!(boolean(&predicate), JsonValue::Bool(true)) {
            results.push(entry.clone());
        }
    }

    Ok(JsonValue::Array(JsonArray::new(results, true, false)))
}

pub async fn single(
    ctx: FunctionContext,
    array: JsonValue,
    predicate: JsonValue,
) -> Result<JsonValue, JsonError> {
    let arr = match array {
        JsonValue::Undefined => return Ok(JsonValue::Undefined),
        JsonValue::Array(arr) => arr,
        _ => {
            return Err(JsonError::new(
                "D3050",
                "$single() expects the first argument to be an array",
            ))
        }
    };

    let callable = match predicate {
        JsonValue::Undefined => None,
        JsonValue::Function(func) => Some(func),
        _ => {
            return Err(JsonError::new(
                "D3050",
                "$single() expects the second argument to be a function",
            ))
        }
    };

    let container = JsonValue::Array(arr.clone());
    let mut found: Option<JsonValue> = None;

    for (index, entry) in arr.elements.iter().enumerate() {
        let matches = if let Some(callable) = &callable {
            let args = build_hof_args(
                callable,
                entry.clone(),
                Some(JsonValue::Number(index as f64)),
                Some(container.clone()),
            );
            let value = callable.call(ctx.clone(), args).await?;
            matches!(boolean(&value), JsonValue::Bool(true))
        } else {
            true
        };

        if matches {
            if found.is_some() {
                return Err(JsonError::new(
                    "D3138",
                    format!(
                        "$single() found more than one matching element (conflict at index {})",
                        index
                    ),
                ));
            }
            found = Some(entry.clone());
        }
    }

    found.ok_or_else(|| {
        JsonError::new(
            "D3139",
            "$single() did not find a matching element in the supplied sequence",
        )
    })
}

pub async fn fold_left(
    ctx: FunctionContext,
    sequence: JsonValue,
    func: JsonValue,
    init: JsonValue,
) -> Result<JsonValue, JsonError> {
    let array = match sequence {
        JsonValue::Undefined => return Ok(JsonValue::Undefined),
        JsonValue::Array(arr) => arr,
        _ => {
            return Err(JsonError::new(
                "D3050",
                "$foldLeft() expects the first argument to be an array",
            ))
        }
    };

    let callable = match func {
        JsonValue::Function(func) => func,
        _ => {
            return Err(JsonError::new(
                "D3050",
                "$foldLeft() expects the second argument to be a function",
            ))
        }
    };

    let arity = callable.arity().unwrap_or(2);
    if arity < 2 {
        return Err(JsonError::new(
            "D3050",
            "$foldLeft() expects the function argument to accept at least two parameters",
        ));
    }

    let mut index;
    let mut accumulator;

    if matches!(init, JsonValue::Undefined) {
        if array.elements.is_empty() {
            return Ok(JsonValue::Undefined);
        }
        accumulator = array.elements[0].clone();
        index = 1;
    } else {
        accumulator = init;
        index = 0;
    }

    let container = JsonValue::Array(array.clone());

    while index < array.elements.len() {
        let mut args = Vec::new();
        args.push(accumulator.clone());
        args.push(array.elements[index].clone());
        if arity >= 3 {
            args.push(JsonValue::Number(index as f64));
        }
        if arity >= 4 {
            args.push(container.clone());
        }
        accumulator = callable.call(ctx.clone(), args).await?;
        index += 1;
    }

    Ok(accumulator)
}

pub async fn each(
    ctx: FunctionContext,
    input: JsonValue,
    func: JsonValue,
) -> Result<JsonValue, JsonError> {
    let callable = match func {
        JsonValue::Function(func) => func,
        _ => {
            return Err(JsonError::new(
                "D3050",
                "$each() expects the second argument to be a function",
            ))
        }
    };

    let mut results: Vec<JsonValue> = Vec::new();

    match input {
        JsonValue::Undefined => return Ok(JsonValue::Undefined),
        JsonValue::Object(object) => {
            let container = JsonValue::Object(object.clone());
            for (key, value) in object.0.iter() {
                let args = build_hof_args(
                    &callable,
                    value.clone(),
                    Some(JsonValue::String(key.clone())),
                    Some(container.clone()),
                );
                let result = callable.call(ctx.clone(), args).await?;
                if !result.is_undefined() {
                    results.push(result);
                }
            }
        }
        JsonValue::Array(array) => {
            let container = JsonValue::Array(array.clone());
            for (index, value) in array.elements.iter().enumerate() {
                let args = build_hof_args(
                    &callable,
                    value.clone(),
                    Some(JsonValue::String(index.to_string())),
                    Some(container.clone()),
                );
                let result = callable.call(ctx.clone(), args).await?;
                if !result.is_undefined() {
                    results.push(result);
                }
            }
        }
        _ => {
            return Err(JsonError::new(
                "D3050",
                "$each() expects the first argument to be an object or array",
            ))
        }
    }

    Ok(JsonValue::Array(JsonArray::new(results, true, false)))
}

pub async fn sift(
    ctx: FunctionContext,
    input: JsonValue,
    func: JsonValue,
) -> Result<JsonValue, JsonError> {
    let callable = match func {
        JsonValue::Function(func) => func,
        _ => {
            return Err(JsonError::new(
                "D3050",
                "$sift() expects the second argument to be a function",
            ))
        }
    };

    match input {
        JsonValue::Undefined => Ok(JsonValue::Undefined),
        JsonValue::Object(JsonObject(entries)) => {
            let container = JsonValue::Object(JsonObject(entries.clone()));
            let mut result: Vec<(String, JsonValue)> = Vec::new();
            for (key, value) in entries {
                let args = build_hof_args(
                    &callable,
                    value.clone(),
                    Some(JsonValue::String(key.clone())),
                    Some(container.clone()),
                );
                let predicate = callable.call(ctx.clone(), args).await?;
                if matches!(boolean(&predicate), JsonValue::Bool(true)) {
                    result.push((key, value));
                }
            }
            if result.is_empty() {
                Ok(JsonValue::Undefined)
            } else {
                Ok(JsonValue::Object(JsonObject(result)))
            }
        }
        JsonValue::Array(array) => {
            let container = JsonValue::Array(array.clone());
            let mut result: Vec<(String, JsonValue)> = Vec::new();
            for (index, value) in array.elements.iter().enumerate() {
                let key = index.to_string();
                let args = build_hof_args(
                    &callable,
                    value.clone(),
                    Some(JsonValue::String(key.clone())),
                    Some(container.clone()),
                );
                let predicate = callable.call(ctx.clone(), args).await?;
                if matches!(boolean(&predicate), JsonValue::Bool(true)) {
                    result.push((key, value.clone()));
                }
            }
            if result.is_empty() {
                Ok(JsonValue::Undefined)
            } else {
                Ok(JsonValue::Object(JsonObject(result)))
            }
        }
        _ => Err(JsonError::new(
            "D3050",
            "$sift() expects the first argument to be an object or array",
        )),
    }
}

pub fn spread(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Undefined => JsonValue::Undefined,
        JsonValue::Array(array) => {
            let mut aggregated: Vec<JsonValue> = Vec::new();
            for element in &array.elements {
                let expanded = spread(element);
                match expanded {
                    JsonValue::Array(JsonArray { elements, .. }) => {
                        aggregated.extend(elements);
                    }
                    other => aggregated.push(other),
                }
            }
            JsonValue::Array(JsonArray::new(aggregated, true, false))
        }
        JsonValue::Object(JsonObject(entries)) => {
            let is_lambda_object = entries.iter().any(|(key, value)| {
                (key == "_jsonata_lambda" || key == "_jsonata_function")
                    && matches!(value, JsonValue::Bool(true))
            });
            if is_lambda_object {
                return JsonValue::Object(JsonObject(entries.clone()));
            }

            let mut aggregated: Vec<JsonValue> = Vec::with_capacity(entries.len());
            for (key, value) in entries {
                aggregated.push(JsonValue::Object(JsonObject(vec![(
                    key.clone(),
                    value.clone(),
                )])));
            }
            JsonValue::Array(JsonArray::new(aggregated, true, false))
        }
        other => other.clone(),
    }
}

pub fn merge(value: &JsonValue) -> Result<JsonValue, JsonError> {
    match value {
        JsonValue::Undefined => Ok(JsonValue::Undefined),
        JsonValue::Array(array) => {
            let mut merged: Vec<(String, JsonValue)> = Vec::new();
            for element in &array.elements {
                if let JsonValue::Object(JsonObject(entries)) = element {
                    for (key, val) in entries {
                        if let Some((_, existing)) = merged
                            .iter_mut()
                            .find(|(existing_key, _)| existing_key == key)
                        {
                            *existing = val.clone();
                        } else {
                            merged.push((key.clone(), val.clone()));
                        }
                    }
                }
            }
            Ok(JsonValue::Object(JsonObject(merged)))
        }
        _ => Err(JsonError::new(
            "D3050",
            "$merge() expects the argument to be an array of objects",
        )),
    }
}

pub fn reverse(value: &JsonValue) -> Result<JsonValue, JsonError> {
    match value {
        JsonValue::Undefined => Ok(JsonValue::Undefined),
        JsonValue::Array(array) => {
            if array.elements.len() <= 1 {
                return Ok(JsonValue::Array(array.clone()));
            }
            let mut elements = array.elements.clone();
            elements.reverse();
            Ok(JsonValue::Array(JsonArray::new(
                elements,
                array.is_sequence,
                array.outer_wrapper,
            )))
        }
        _ => Err(JsonError::new(
            "D3050",
            "$reverse() expects the argument to be an array",
        )),
    }
}

pub fn shuffle(value: &JsonValue) -> Result<JsonValue, JsonError> {
    match value {
        JsonValue::Undefined => Ok(JsonValue::Undefined),
        JsonValue::Array(array) => {
            if array.elements.len() <= 1 {
                return Ok(JsonValue::Array(array.clone()));
            }
            let mut rng = rand::rng();
            let mut shuffled = vec![JsonValue::Undefined; array.elements.len()];
            for (index, element) in array.elements.iter().enumerate() {
                let j = rng.random_range(0..=index);
                if index != j {
                    shuffled[index] = shuffled[j].clone();
                }
                shuffled[j] = element.clone();
            }
            Ok(JsonValue::Array(JsonArray::new(
                shuffled,
                array.is_sequence,
                array.outer_wrapper,
            )))
        }
        _ => Err(JsonError::new(
            "D3050",
            "$shuffle() expects the argument to be an array",
        )),
    }
}

pub fn distinct(value: &JsonValue) -> Result<JsonValue, JsonError> {
    match value {
        JsonValue::Undefined => Ok(JsonValue::Undefined),
        JsonValue::Array(array) => {
            if array.elements.len() <= 1 {
                return Ok(JsonValue::Array(array.clone()));
            }
            let mut results: Vec<JsonValue> = Vec::new();
            for element in &array.elements {
                if !results.iter().any(|existing| existing == element) {
                    results.push(element.clone());
                }
            }
            Ok(JsonValue::Array(JsonArray::new(
                results,
                array.is_sequence,
                array.outer_wrapper,
            )))
        }
        _ => Ok(value.clone()),
    }
}

pub fn assert(condition: &JsonValue, message: Option<&JsonValue>) -> Result<JsonValue, JsonError> {
    if matches!(boolean(condition), JsonValue::Bool(true)) {
        return Ok(JsonValue::Undefined);
    }

    let message_text = if let Some(msg) = message {
        match msg {
            JsonValue::String(text) => text.clone(),
            _ => match strings::string(msg, false) {
                Ok(JsonValue::String(text)) => text,
                _ => "$assert() statement failed".to_owned(),
            },
        }
    } else {
        "$assert() statement failed".to_owned()
    };

    Err(JsonError::new("D3141", message_text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::JsonCallable;
    use futures::executor::block_on;
    use futures::future::BoxFuture;
    use std::any::Any;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct RecordingCallable {
        arity: usize,
        calls: Arc<Mutex<Vec<Vec<JsonValue>>>>,
    }

    impl RecordingCallable {
        fn new(arity: usize) -> Self {
            Self {
                arity,
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn calls(&self) -> Arc<Mutex<Vec<Vec<JsonValue>>>> {
            Arc::clone(&self.calls)
        }
    }

    impl JsonCallable for RecordingCallable {
        fn call(
            &self,
            _ctx: FunctionContext,
            args: Vec<JsonValue>,
        ) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
            let captured = args.clone();
            let output = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let store = self.calls();
            Box::pin(async move {
                if let Ok(mut guard) = store.lock() {
                    guard.push(captured);
                }
                Ok(output)
            })
        }

        fn arity(&self) -> Option<usize> {
            Some(self.arity)
        }

        fn as_any(&self) -> &(dyn Any + Send + Sync) {
            self
        }
    }

    #[derive(Clone)]
    struct PredicateCallable;

    impl JsonCallable for PredicateCallable {
        fn call(
            &self,
            _ctx: FunctionContext,
            args: Vec<JsonValue>,
        ) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
            let value = args.first().cloned().unwrap_or(JsonValue::Undefined);
            let result = matches!(value, JsonValue::Number(number) if number > 1.0);
            Box::pin(async move { Ok(JsonValue::Bool(result)) })
        }

        fn arity(&self) -> Option<usize> {
            Some(1)
        }

        fn as_any(&self) -> &(dyn Any + Send + Sync) {
            self
        }
    }

    #[derive(Clone)]
    struct SumCallable;

    impl JsonCallable for SumCallable {
        fn call(
            &self,
            _ctx: FunctionContext,
            args: Vec<JsonValue>,
        ) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
            let left = match args.get(0) {
                Some(JsonValue::Number(value)) => *value,
                _ => 0.0,
            };
            let right = match args.get(1) {
                Some(JsonValue::Number(value)) => *value,
                _ => 0.0,
            };
            Box::pin(async move { Ok(JsonValue::Number(left + right)) })
        }

        fn arity(&self) -> Option<usize> {
            Some(2)
        }

        fn as_any(&self) -> &(dyn Any + Send + Sync) {
            self
        }
    }

    #[test]
    fn boolean_handles_primitives() {
        assert_eq!(boolean(&JsonValue::Undefined), JsonValue::Undefined);
        assert_eq!(boolean(&JsonValue::Null), JsonValue::Bool(false));
        assert_eq!(boolean(&JsonValue::Bool(true)), JsonValue::Bool(true));
        assert_eq!(boolean(&JsonValue::Number(0.0)), JsonValue::Bool(false));
        assert_eq!(boolean(&JsonValue::Number(42.0)), JsonValue::Bool(true));
        assert_eq!(
            boolean(&JsonValue::String(String::from(""))),
            JsonValue::Bool(false)
        );
        assert_eq!(
            boolean(&JsonValue::String(String::from("x"))),
            JsonValue::Bool(true)
        );
    }

    #[test]
    fn boolean_handles_arrays() {
        let empty = JsonValue::Array(JsonArray::empty_sequence());
        assert_eq!(boolean(&empty), JsonValue::Bool(false));

        let single_undefined =
            JsonValue::Array(JsonArray::new(vec![JsonValue::Undefined], true, false));
        assert_eq!(boolean(&single_undefined), JsonValue::Undefined);

        let nested = JsonValue::Array(JsonArray::new(
            vec![
                JsonValue::Bool(false),
                JsonValue::Bool(true),
                JsonValue::Bool(false),
            ],
            true,
            false,
        ));
        assert_eq!(boolean(&nested), JsonValue::Bool(true));
    }

    #[test]
    fn boolean_handles_objects() {
        let empty = JsonValue::Object(JsonObject(vec![]));
        assert_eq!(boolean(&empty), JsonValue::Bool(false));

        let non_empty = JsonValue::Object(JsonObject(vec![(
            "key".to_string(),
            JsonValue::Number(1.0),
        )]));
        assert_eq!(boolean(&non_empty), JsonValue::Bool(true));
    }

    #[test]
    fn type_of_reports_expected_tags() {
        assert_eq!(type_of(&JsonValue::Undefined), JsonValue::Undefined);
        assert_eq!(
            type_of(&JsonValue::Null),
            JsonValue::String("null".to_owned())
        );
        assert_eq!(
            type_of(&JsonValue::Bool(true)),
            JsonValue::String("boolean".to_owned())
        );
        assert_eq!(
            type_of(&JsonValue::Number(0.0)),
            JsonValue::String("number".to_owned())
        );
        assert_eq!(
            type_of(&JsonValue::String("value".to_owned())),
            JsonValue::String("string".to_owned())
        );
        assert_eq!(
            type_of(&JsonValue::Array(JsonArray::empty_sequence())),
            JsonValue::String("array".to_owned())
        );
        assert_eq!(
            type_of(&JsonValue::Object(JsonObject(Vec::new()))),
            JsonValue::String("object".to_owned())
        );

        let callable = RecordingCallable::new(0);
        let function_value = JsonValue::Function(JsonFunction::new(Arc::new(callable)));
        assert_eq!(
            type_of(&function_value),
            JsonValue::String("function".to_owned())
        );
    }

    #[test]
    fn not_inverts_boolean() {
        assert_eq!(not(&JsonValue::Bool(true)), JsonValue::Bool(false));
        assert_eq!(not(&JsonValue::Bool(false)), JsonValue::Bool(true));

        let undefined = JsonValue::Undefined;
        assert_eq!(not(&undefined), JsonValue::Undefined);
    }

    #[test]
    fn zip_combines_arrays_by_index() {
        let left = JsonValue::Array(JsonArray::new(
            vec![
                JsonValue::Number(1.0),
                JsonValue::Number(2.0),
                JsonValue::Number(3.0),
            ],
            true,
            false,
        ));
        let right = JsonValue::Array(JsonArray::new(
            vec![
                JsonValue::Number(4.0),
                JsonValue::Number(5.0),
                JsonValue::Number(6.0),
            ],
            true,
            false,
        ));

        let result = zip(&[left, right]);
        if let JsonValue::Array(JsonArray { elements, .. }) = result {
            assert_eq!(elements.len(), 3);
            assert!(matches!(elements[0], JsonValue::Array(_)));
        } else {
            panic!("Expected array result from zip");
        }
    }

    #[test]
    fn zip_handles_scalars() {
        let result = zip(&[
            JsonValue::Number(1.0),
            JsonValue::Number(2.0),
            JsonValue::Number(3.0),
        ]);
        if let JsonValue::Array(JsonArray { elements, .. }) = result {
            assert_eq!(elements.len(), 1);
        } else {
            panic!("Expected array result from zip");
        }
    }

    #[test]
    fn sort_default_orders_numbers() {
        let arr = JsonArray::new(
            vec![
                JsonValue::Number(3.0),
                JsonValue::Number(1.0),
                JsonValue::Number(2.0),
            ],
            true,
            false,
        );
        let sorted = sort_default(&JsonValue::Array(arr)).expect("sort should succeed");
        if let JsonValue::Array(JsonArray { elements, .. }) = sorted {
            let values: Vec<f64> = elements
                .into_iter()
                .map(|v| match v {
                    JsonValue::Number(n) => n,
                    _ => panic!("Expected number"),
                })
                .collect();
            assert_eq!(values, vec![1.0, 2.0, 3.0]);
        } else {
            panic!("Expected array result from sort");
        }
    }

    #[test]
    fn sort_default_rejects_mixed_types() {
        let arr = JsonArray::new(
            vec![JsonValue::Number(1.0), JsonValue::String("a".into())],
            true,
            false,
        );
        assert!(sort_default(&JsonValue::Array(arr)).is_err());
    }

    #[test]
    fn map_applies_callable_respecting_arity() {
        let callable = RecordingCallable::new(2);
        let function = JsonFunction::new(Arc::new(callable.clone()));
        let array = JsonArray::new(
            vec![JsonValue::Number(1.0), JsonValue::Number(2.0)],
            true,
            false,
        );
        let result = block_on(map(
            FunctionContext::empty(),
            JsonValue::Array(array),
            JsonValue::Function(function),
        ))
        .expect("map should succeed");

        match result {
            JsonValue::Array(JsonArray { elements, .. }) => {
                assert_eq!(elements.len(), 2);
            }
            other => panic!("Expected array, got {:?}", other),
        }

        let calls = callable.calls();
        let stored = calls.lock().unwrap();
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].len(), 2);
        assert!(matches!(stored[0][1], JsonValue::Number(0.0)));
        assert!(matches!(stored[1][1], JsonValue::Number(1.0)));
    }

    #[test]
    fn each_iterates_objects_and_arrays() {
        let callable = RecordingCallable::new(3);
        let function = JsonFunction::new(Arc::new(callable.clone()));
        let object = JsonObject(vec![
            ("a".to_string(), JsonValue::Number(10.0)),
            ("b".to_string(), JsonValue::Number(20.0)),
        ]);

        let object_result = block_on(each(
            FunctionContext::empty(),
            JsonValue::Object(object.clone()),
            JsonValue::Function(function.clone()),
        ))
        .expect("each should succeed for objects");

        match object_result {
            JsonValue::Array(JsonArray { elements, .. }) => {
                assert_eq!(elements.len(), 2);
            }
            other => panic!("Expected array from object iteration, got {:?}", other),
        }

        let array = JsonArray::new(
            vec![JsonValue::String("x".into()), JsonValue::String("y".into())],
            true,
            false,
        );

        let array_result = block_on(each(
            FunctionContext::empty(),
            JsonValue::Array(array),
            JsonValue::Function(function),
        ))
        .expect("each should succeed for arrays");

        match array_result {
            JsonValue::Array(JsonArray { elements, .. }) => {
                assert_eq!(elements.len(), 2);
            }
            other => panic!("Expected array from array iteration, got {:?}", other),
        }

        let calls = callable.calls();
        let stored = calls.lock().unwrap();
        assert_eq!(stored.len(), 4);
        assert!(matches!(
            stored[0][1],
            JsonValue::String(ref key) if key == "a"
        ));
        assert!(matches!(
            stored[2][1],
            JsonValue::String(ref key) if key == "0"
        ));
    }

    #[test]
    fn filter_selects_matching_entries() {
        let predicate = JsonValue::Function(JsonFunction::new(Arc::new(PredicateCallable)));
        let array = JsonValue::Array(JsonArray::new(
            vec![
                JsonValue::Number(0.0),
                JsonValue::Number(2.0),
                JsonValue::Number(3.0),
            ],
            true,
            false,
        ));
        let result = block_on(filter(FunctionContext::empty(), array, predicate)).unwrap();
        if let JsonValue::Array(JsonArray { elements, .. }) = result {
            assert_eq!(
                elements,
                vec![JsonValue::Number(2.0), JsonValue::Number(3.0)]
            );
        } else {
            panic!("Expected filtered sequence");
        }
    }

    #[test]
    fn single_returns_unique_match() {
        let array = JsonValue::Array(JsonArray::new(
            vec![JsonValue::String("value".to_owned())],
            true,
            false,
        ));
        let result = block_on(single(
            FunctionContext::empty(),
            array,
            JsonValue::Undefined,
        ))
        .unwrap();
        assert_eq!(result, JsonValue::String("value".to_owned()));
    }

    #[test]
    fn single_detects_multiple_matches() {
        let array = JsonValue::Array(JsonArray::new(
            vec![JsonValue::Number(2.0), JsonValue::Number(3.0)],
            true,
            false,
        ));
        let predicate = JsonValue::Function(JsonFunction::new(Arc::new(PredicateCallable)));
        let error = block_on(single(FunctionContext::empty(), array, predicate)).unwrap_err();
        assert_eq!(error.code, "D3138");
    }

    #[test]
    fn fold_left_accumulates_values() {
        let array = JsonValue::Array(JsonArray::new(
            vec![JsonValue::Number(1.0), JsonValue::Number(2.0)],
            true,
            false,
        ));
        let func = JsonValue::Function(JsonFunction::new(Arc::new(SumCallable)));
        let total = block_on(fold_left(
            FunctionContext::empty(),
            array,
            func,
            JsonValue::Number(1.0),
        ))
        .unwrap();
        assert_eq!(total, JsonValue::Number(4.0));
    }

    #[test]
    fn sift_filters_object_properties() {
        let object = JsonValue::Object(JsonObject(vec![
            ("a".to_owned(), JsonValue::Number(1.0)),
            ("b".to_owned(), JsonValue::Number(3.0)),
        ]));
        let predicate = JsonValue::Function(JsonFunction::new(Arc::new(PredicateCallable)));
        let result = block_on(sift(FunctionContext::empty(), object, predicate)).unwrap();
        if let JsonValue::Object(JsonObject(entries)) = result {
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].0, "b");
        } else {
            panic!("Expected sifted object");
        }
    }

    #[test]
    fn spread_expands_object_into_sequence() {
        let object = JsonValue::Object(JsonObject(vec![
            ("a".to_owned(), JsonValue::Number(1.0)),
            ("b".to_owned(), JsonValue::Number(2.0)),
        ]));
        let result = spread(&object);
        if let JsonValue::Array(JsonArray { elements, .. }) = result {
            assert_eq!(elements.len(), 2);
        } else {
            panic!("Expected spread sequence");
        }
    }

    #[test]
    fn merge_combines_objects() {
        let array = JsonValue::Array(JsonArray::new(
            vec![
                JsonValue::Object(JsonObject(vec![("a".to_owned(), JsonValue::Number(1.0))])),
                JsonValue::Object(JsonObject(vec![("b".to_owned(), JsonValue::Number(2.0))])),
            ],
            true,
            false,
        ));
        let merged = merge(&array).unwrap();
        if let JsonValue::Object(JsonObject(entries)) = merged {
            assert_eq!(entries.len(), 2);
        } else {
            panic!("Expected merged object");
        }
    }

    #[test]
    fn reverse_inverts_array_order() {
        let array = JsonValue::Array(JsonArray::new(
            vec![
                JsonValue::Number(1.0),
                JsonValue::Number(2.0),
                JsonValue::Number(3.0),
            ],
            true,
            false,
        ));
        let reversed = reverse(&array).unwrap();
        if let JsonValue::Array(JsonArray { elements, .. }) = reversed {
            assert_eq!(
                elements,
                vec![
                    JsonValue::Number(3.0),
                    JsonValue::Number(2.0),
                    JsonValue::Number(1.0)
                ]
            );
        } else {
            panic!("Expected reversed array");
        }
    }

    #[test]
    fn shuffle_preserves_elements() {
        let array = JsonValue::Array(JsonArray::new(
            vec![
                JsonValue::Number(1.0),
                JsonValue::Number(2.0),
                JsonValue::Number(3.0),
            ],
            true,
            false,
        ));
        let shuffled = shuffle(&array).unwrap();
        if let JsonValue::Array(JsonArray { mut elements, .. }) = shuffled {
            let mut actual_numbers: Vec<f64> = elements
                .drain(..)
                .map(|value| match value {
                    JsonValue::Number(number) => number,
                    other => panic!("Expected shuffled number, got {:?}", other),
                })
                .collect();
            actual_numbers.sort_by(|left, right| left.total_cmp(right));
            assert_eq!(actual_numbers, vec![1.0, 2.0, 3.0]);
        } else {
            panic!("Expected shuffled array");
        }
    }

    #[test]
    fn distinct_eliminates_duplicates() {
        let array = JsonValue::Array(JsonArray::new(
            vec![
                JsonValue::Number(1.0),
                JsonValue::Number(1.0),
                JsonValue::Number(2.0),
            ],
            true,
            false,
        ));
        let distinct_values = distinct(&array).unwrap();
        if let JsonValue::Array(JsonArray { elements, .. }) = distinct_values {
            assert_eq!(
                elements,
                vec![JsonValue::Number(1.0), JsonValue::Number(2.0)]
            );
        } else {
            panic!("Expected array of distinct values");
        }
    }

    #[test]
    fn assert_returns_error_when_condition_false() {
        let error = assert(&JsonValue::Bool(false), None).unwrap_err();
        assert_eq!(error.code, "D3141");
    }

    #[test]
    fn assert_returns_undefined_when_condition_true() {
        let value = assert(&JsonValue::Bool(true), None).unwrap();
        assert_eq!(value, JsonValue::Undefined);
    }
}
