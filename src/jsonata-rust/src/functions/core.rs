use std::cmp::Ordering;
use std::collections::HashSet;

use crate::types::{
    FunctionContext, JsonArray, JsonError, JsonFunction, JsonObject, JsonValue,
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
        JsonValue::Function(_) => JsonValue::Bool(true),
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

fn build_hof_args(
    callable: &JsonFunction,
    first: JsonValue,
    second: Option<JsonValue>,
    third: Option<JsonValue>,
) -> Vec<JsonValue> {
    let arity = callable.arity().unwrap_or(3).max(1);

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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use futures::future::BoxFuture;
    use crate::types::JsonCallable;
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
            vec![
                JsonValue::String("x".into()),
                JsonValue::String("y".into()),
            ],
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
}
