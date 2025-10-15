use std::cmp::Ordering;
use std::collections::HashSet;

use crate::types::{JsonArray, JsonError, JsonObject, JsonValue};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
