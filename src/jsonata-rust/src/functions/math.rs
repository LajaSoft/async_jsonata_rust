use rand::Rng;

use crate::types::{JsonError, JsonObject, JsonValue, JsonataValue};

/// Numeric helper functions translated from the JSONata JavaScript implementation.
pub fn normalize_js_number(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    if value == 0.0 {
        return 0.0;
    }
    let abs = value.abs();
    let exponent = abs.log10().floor();
    let scale = 10_f64.powf(15.0 - 1.0 - exponent);
    let rounded = (value * scale).round() / scale;
    if rounded == 0.0 {
        0.0
    } else {
        rounded
    }
}

pub fn sum(args: Option<&[f64]>) -> Option<f64> {
    let slice = args?;
    let total: f64 = slice.iter().copied().sum();
    Some(total)
}

fn jsonata_value_to_number(value: &JsonataValue) -> Option<f64> {
    match value {
        JsonataValue::Undefined => None,
        JsonataValue::Null => Some(0.0),
        JsonataValue::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        JsonataValue::Number(num) => Some(*num),
        JsonataValue::String(text) => text.parse::<f64>().ok(),
        JsonataValue::Array(_)
        | JsonataValue::Object(_)
        | JsonataValue::Function(_)
        | JsonataValue::NativeRef(_) => None,
    }
}

pub fn sum_jsonata(value: &JsonataValue) -> Result<JsonataValue, JsonError> {
    match value {
        JsonataValue::Undefined | JsonataValue::Null => Ok(JsonataValue::Undefined),
        JsonataValue::Array(array) => {
            if array.elements.is_empty() {
                return Ok(JsonataValue::Undefined);
            }
            let mut total = 0.0;
            let mut found = false;
            for element in &array.elements {
                if let Some(num) = jsonata_value_to_number(element) {
                    total += num;
                    found = true;
                } else {
                    return Err(JsonError::new(
                        "D3050",
                        "$sum() expects the input array to contain only numeric values",
                    ));
                }
            }
            if !found {
                return Ok(JsonataValue::Undefined);
            }
            Ok(JsonataValue::Number(total))
        }
        other => {
            if let Some(num) = jsonata_value_to_number(other) {
                Ok(JsonataValue::Number(num))
            } else {
                Err(JsonError::new(
                    "D3050",
                    "$sum() expects a numeric argument or an array of numerics",
                ))
            }
        }
    }
}

pub fn count<T>(args: Option<&[T]>) -> usize {
    match args {
        Some(slice) => slice.len(),
        None => 0,
    }
}

/// Collects the numeric elements of a JSONata `$max`/`$min` argument.
///
/// A single number is treated as a singleton array. Every element must be a
/// number; otherwise a `T0412` error is raised. An empty array (or `undefined`)
/// yields `Ok(None)`, which the caller maps to `undefined`.
fn collect_numbers(value: &JsonValue) -> Result<Option<Vec<f64>>, JsonError> {
    match value {
        // The `undefined` literal is surfaced to builtins as `Null` by the
        // evaluator; both mean "no value" for $max/$min and yield undefined.
        JsonValue::Undefined | JsonValue::Null => Ok(None),
        JsonValue::Number(num) => Ok(Some(vec![*num])),
        JsonValue::Array(array) => {
            if array.elements.is_empty() {
                return Ok(None);
            }
            let mut numbers = Vec::with_capacity(array.elements.len());
            for element in &array.elements {
                match element {
                    JsonValue::Number(num) => numbers.push(*num),
                    _ => {
                        return Err(JsonError::new(
                            "T0412",
                            "Argument of function must be an array of numbers",
                        ));
                    }
                }
            }
            Ok(Some(numbers))
        }
        // The literal `undefined` is represented by the parser as the marker
        // object `{"__jsonata_undefined__": true}`; treat it as no value.
        JsonValue::Object(JsonObject(entries))
            if entries.len() == 1 && entries[0].0 == "__jsonata_undefined__" =>
        {
            Ok(None)
        }
        _ => Err(JsonError::new(
            "T0412",
            "Argument of function must be an array of numbers",
        )),
    }
}

pub fn max(value: &JsonValue) -> Result<JsonValue, JsonError> {
    match collect_numbers(value)? {
        None => Ok(JsonValue::Undefined),
        Some(numbers) => match numbers.into_iter().reduce(f64::max) {
            Some(result) => Ok(JsonValue::Number(result)),
            None => Ok(JsonValue::Undefined),
        },
    }
}

pub fn min(value: &JsonValue) -> Result<JsonValue, JsonError> {
    match collect_numbers(value)? {
        None => Ok(JsonValue::Undefined),
        Some(numbers) => match numbers.into_iter().reduce(f64::min) {
            Some(result) => Ok(JsonValue::Number(result)),
            None => Ok(JsonValue::Undefined),
        },
    }
}

/// Collects the numeric elements of a `$sum`/`$average` argument.
///
/// `undefined` yields `Ok(None)` (caller maps to undefined). A single number is
/// treated as a singleton. Every element of an array must be a number; otherwise
/// `T0412` is raised. An empty array yields `Ok(None)`.
/// Returns true if the value is the parser's `undefined` literal marker object
/// `{"__jsonata_undefined__": true}`, which the evaluator surfaces for the
/// `undefined` keyword.
fn is_undefined_marker(value: &JsonValue) -> bool {
    matches!(
        value,
        JsonValue::Object(JsonObject(entries))
            if entries.len() == 1 && entries[0].0 == "__jsonata_undefined__"
    )
}

fn collect_numeric_sequence(value: &JsonValue) -> Result<Option<Vec<f64>>, JsonError> {
    if is_undefined_marker(value) {
        return Ok(None);
    }
    match value {
        JsonValue::Undefined | JsonValue::Null => Ok(None),
        JsonValue::Number(num) => Ok(Some(vec![*num])),
        JsonValue::Array(array) => {
            if array.elements.is_empty() {
                return Ok(None);
            }
            let mut numbers = Vec::with_capacity(array.elements.len());
            for element in &array.elements {
                match element {
                    JsonValue::Number(num) => numbers.push(*num),
                    _ => {
                        return Err(JsonError::new(
                            "T0412",
                            "Argument of function must be an array of numbers",
                        ));
                    }
                }
            }
            Ok(Some(numbers))
        }
        // A non-array, non-numeric scalar (e.g. an object substituted as the
        // implicit context for `$sum()`) does not match the `<a<n>>` parameter
        // type, so it is a signature mismatch (T0410) rather than a bad array
        // element (T0412).
        _ => Err(JsonError::new(
            "T0410",
            "Argument of function does not match function signature",
        )),
    }
}

pub fn sum_value(value: &JsonValue) -> Result<JsonValue, JsonError> {
    match collect_numeric_sequence(value)? {
        None => Ok(JsonValue::Undefined),
        Some(numbers) => {
            let total: f64 = numbers.iter().copied().sum();
            Ok(JsonValue::Number(normalize_js_number(total)))
        }
    }
}

pub fn average(value: &JsonValue) -> Result<JsonValue, JsonError> {
    match collect_numeric_sequence(value)? {
        None => Ok(JsonValue::Undefined),
        Some(numbers) => {
            let total: f64 = numbers.iter().copied().sum();
            Ok(JsonValue::Number(normalize_js_number(
                total / numbers.len() as f64,
            )))
        }
    }
}

fn adjust_negative_zero(value: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        value
    }
}

fn shift_decimal(value: f64, shift: f64) -> f64 {
    if shift == 0.0 || !value.is_finite() {
        return value;
    }

    let value_string = value.to_string();
    let mut parts = value_string.split('e');
    let base = parts.next().unwrap_or("0");
    let exponent = parts
        .next()
        .and_then(|part| part.parse::<f64>().ok())
        .unwrap_or(0.0);
    let new_exponent = exponent + shift;
    let combined = format!("{}e{}", base, new_exponent);
    combined
        .parse::<f64>()
        .unwrap_or(value * 10_f64.powf(shift))
}

pub fn random() -> f64 {
    let mut rng = rand::rng();
    rng.random::<f64>()
}

fn parse_radix_string(text: &str) -> Option<f64> {
    if text.len() <= 2 {
        return None;
    }
    let (radix, digits) =
        if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            (16, rest)
        } else if let Some(rest) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
            (8, rest)
        } else if let Some(rest) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
            (2, rest)
        } else {
            return None;
        };

    if digits.is_empty() {
        return None;
    }

    u64::from_str_radix(digits, radix)
        .ok()
        .map(|value| value as f64)
}

fn matches_decimal_pattern(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return false;
    }

    let mut index = 0;
    if bytes[index] == b'-' {
        index += 1;
    }

    let start_digits = index;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index == start_digits {
        return false;
    }

    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        let fraction_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == fraction_start {
            return false;
        }
    }

    if index < bytes.len() && (bytes[index] == b'e' || bytes[index] == b'E') {
        index += 1;
        if index < bytes.len() && (bytes[index] == b'+' || bytes[index] == b'-') {
            index += 1;
        }
        let exponent_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == exponent_start {
            return false;
        }
    }

    index == bytes.len()
}

pub fn number(value: &JsonValue) -> Result<JsonValue, JsonError> {
    match value {
        JsonValue::Undefined => Ok(JsonValue::Undefined),
        JsonValue::Number(num) => Ok(JsonValue::Number(*num)),
        JsonValue::String(text) => {
            if let Some(parsed) = parse_radix_string(text).or_else(|| {
                if matches_decimal_pattern(text) {
                    text.parse::<f64>().ok()
                } else {
                    None
                }
            }) {
                if !parsed.is_finite() {
                    return Err(JsonError::new(
                        "D3030",
                        format!("Unable to cast '{}' to a number", text),
                    ));
                }
                Ok(JsonValue::Number(parsed))
            } else {
                Err(JsonError::new(
                    "D3030",
                    format!("Unable to cast '{}' to a number", text),
                ))
            }
        }
        JsonValue::Bool(flag) => Ok(JsonValue::Number(if *flag { 1.0 } else { 0.0 })),
        _ => Err(JsonError::new("D3030", "Unable to cast value to a number")),
    }
}

pub fn abs(value: Option<f64>) -> Option<f64> {
    value.map(|v| v.abs())
}

pub fn floor(value: Option<f64>) -> Option<f64> {
    value.map(|v| v.floor())
}

pub fn ceil(value: Option<f64>) -> Option<f64> {
    value.map(|v| v.ceil())
}

pub fn round(value: Option<f64>, precision: Option<f64>) -> Option<f64> {
    fn math_round_js(input: f64) -> f64 {
        (input + 0.5).floor()
    }

    let value = value?;
    let precision = precision.filter(|p| *p != 0.0 && !p.is_nan());

    let mut shifted = value;
    if let Some(p) = precision {
        shifted = shift_decimal(value, p);
    }

    let mut result = math_round_js(shifted);
    let diff = result - shifted;
    if diff.abs() == 0.5 && (result % 2.0).abs() == 1.0 {
        result -= 1.0;
    }

    if let Some(p) = precision {
        result = shift_decimal(result, -p);
    }

    Some(adjust_negative_zero(result))
}

pub fn sqrt(value: Option<f64>) -> Result<Option<f64>, JsonError> {
    let value = match value {
        Some(v) => v,
        None => return Ok(None),
    };

    if value < 0.0 {
        return Err(JsonError::new(
            "D3060",
            format!("Square root is not defined for {}", value),
        ));
    }

    Ok(Some(value.sqrt()))
}

pub fn power(base: Option<f64>, exponent: Option<f64>) -> Result<Option<f64>, JsonError> {
    let base = match base {
        Some(v) => v,
        None => return Ok(None),
    };

    let exponent_value = exponent.unwrap_or(f64::NAN);
    let result = base.powf(exponent_value);

    if !result.is_finite() {
        return Err(JsonError::new(
            "D3061",
            format!(
                "Result of {} raised to the power of {} is not finite",
                base, exponent_value
            ),
        ));
    }

    Ok(Some(result))
}

pub fn count_value(value: &JsonValue) -> JsonValue {
    // The `<a:n>` signature coerces undefined to an empty array and any
    // non-array (scalar) value to a singleton, so $count always counts the
    // number of items in the resulting array.
    if is_undefined_marker(value) {
        return JsonValue::Number(0.0);
    }
    match value {
        JsonValue::Undefined => JsonValue::Number(0.0),
        JsonValue::Array(array) => JsonValue::Number(array.elements.len() as f64),
        _ => JsonValue::Number(1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sum_handles_none() {
        assert_eq!(sum(None), None);
    }

    #[test]
    fn sum_handles_values() {
        let data = [1.0, 2.0, 3.0];
        assert_eq!(sum(Some(&data)), Some(6.0));
    }

    #[test]
    fn count_defaults_to_zero() {
        assert_eq!(count::<u8>(None), 0);
    }

    #[test]
    fn count_counts_items() {
        let data = [1, 2, 3, 4];
        assert_eq!(count(Some(&data)), 4);
    }

    fn num_array(values: &[f64]) -> JsonValue {
        JsonValue::Array(crate::types::JsonArray::new(
            values.iter().map(|v| JsonValue::Number(*v)).collect(),
            false,
            false,
        ))
    }

    #[test]
    fn max_respects_empty() {
        assert_eq!(max(&num_array(&[])).unwrap(), JsonValue::Undefined);
        assert_eq!(max(&JsonValue::Undefined).unwrap(), JsonValue::Undefined);
    }

    #[test]
    fn max_finds_value() {
        assert_eq!(
            max(&num_array(&[1.0, 3.5, 2.0])).unwrap(),
            JsonValue::Number(3.5)
        );
        assert_eq!(max(&JsonValue::Number(7.0)).unwrap(), JsonValue::Number(7.0));
    }

    #[test]
    fn min_finds_value() {
        assert_eq!(
            min(&num_array(&[1.0, 3.5, 2.0])).unwrap(),
            JsonValue::Number(1.0)
        );
    }

    #[test]
    fn max_rejects_non_numbers() {
        let mixed = JsonValue::Array(crate::types::JsonArray::new(
            vec![JsonValue::String("1".to_string()), JsonValue::Number(2.0)],
            false,
            false,
        ));
        let err = max(&mixed).unwrap_err();
        assert_eq!(err.code, "T0412");
    }

    #[test]
    fn average_requires_items() {
        assert_eq!(
            average(&num_array(&[2.0, 4.0])).unwrap(),
            JsonValue::Number(3.0)
        );
        assert_eq!(average(&num_array(&[])).unwrap(), JsonValue::Undefined);
        assert_eq!(average(&JsonValue::Undefined).unwrap(), JsonValue::Undefined);
    }

    #[test]
    fn average_rejects_non_numbers() {
        let mixed = JsonValue::Array(crate::types::JsonArray::new(
            vec![JsonValue::String("1".to_string()), JsonValue::Number(2.0)],
            false,
            false,
        ));
        assert_eq!(average(&mixed).unwrap_err().code, "T0412");
    }

    #[test]
    fn random_in_range() {
        let value = random();
        assert!(value >= 0.0 && value < 1.0);
    }

    #[test]
    fn abs_and_floor_behave() {
        assert_eq!(abs(Some(-10.5)), Some(10.5));
        assert_eq!(floor(Some(3.7)), Some(3.0));
        assert_eq!(ceil(Some(3.2)), Some(4.0));
    }

    #[test]
    fn round_half_even_matches_expectations() {
        assert_eq!(round(Some(2.5), None), Some(2.0));
        assert_eq!(round(Some(3.5), None), Some(4.0));
        assert_eq!(round(Some(2.345), Some(2.0)), Some(2.34));
    }

    #[test]
    fn sqrt_errors_for_negative_numbers() {
        assert!(sqrt(Some(-1.0)).is_err());
        assert_eq!(sqrt(Some(9.0)).unwrap(), Some(3.0));
    }

    #[test]
    fn power_detects_non_finite_results() {
        assert!(power(Some(0.0), Some(-1.0)).is_err());
        assert_eq!(power(Some(2.0), Some(3.0)).unwrap(), Some(8.0));
    }

    #[test]
    fn number_parses_strings_and_radix() {
        assert_eq!(
            number(&JsonValue::String("42".to_owned())).unwrap(),
            JsonValue::Number(42.0)
        );
        assert_eq!(
            number(&JsonValue::String("-3.14".to_owned())).unwrap(),
            JsonValue::Number(-3.14)
        );
        assert_eq!(
            number(&JsonValue::String("0x10".to_owned())).unwrap(),
            JsonValue::Number(16.0)
        );
        assert_eq!(
            number(&JsonValue::Bool(true)).unwrap(),
            JsonValue::Number(1.0)
        );
    }

    #[test]
    fn number_rejects_invalid_values() {
        assert!(number(&JsonValue::String("12abc".to_owned())).is_err());
        assert!(number(&JsonValue::Object(JsonObject(vec![]))).is_err());
    }

    #[test]
    fn count_value_handles_sequences_and_strings() {
        let array = JsonValue::sequence(vec![JsonValue::Number(1.0), JsonValue::Number(2.0)]);
        assert_eq!(count_value(&array), JsonValue::Number(2.0));

        let text = JsonValue::String("hello".to_string());
        assert_eq!(count_value(&text), JsonValue::Number(1.0));

        assert_eq!(count_value(&JsonValue::Undefined), JsonValue::Number(0.0));
    }
}
