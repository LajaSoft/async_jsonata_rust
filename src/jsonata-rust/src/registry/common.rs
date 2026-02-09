use time::{OffsetDateTime, UtcOffset};

use crate::types::{JsonError, JsonObject, JsonValue};

pub(super) fn json_value_to_number(value: &JsonValue) -> Option<f64> {
    match value {
        JsonValue::Number(n) => Some(*n),
        JsonValue::Bool(true) => Some(1.0),
        JsonValue::Bool(false) => Some(0.0),
        JsonValue::String(s) => s.parse().ok(),
        _ => None,
    }
}

pub(super) fn number_to_json_value(value: Option<f64>) -> JsonValue {
    match value {
        Some(n) => JsonValue::Number(n),
        None => JsonValue::Undefined,
    }
}

fn parse_timezone_offset(value: &str) -> Option<UtcOffset> {
    if value.len() != 5 {
        return None;
    }

    let sign = match &value[0..1] {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };
    let hours: i8 = value[1..3].parse().ok()?;
    let minutes: i8 = value[3..5].parse().ok()?;
    let hours = hours.saturating_mul(sign);
    let minutes = minutes.saturating_mul(sign);
    UtcOffset::from_hms(hours, minutes, 0).ok()
}

pub(super) fn format_now_iso_utc(now: OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        now.millisecond()
    )
}

pub(super) fn format_now_custom(now: OffsetDateTime, timezone: Option<&str>) -> String {
    let offset = timezone
        .and_then(parse_timezone_offset)
        .unwrap_or(UtcOffset::UTC);
    let localized = now.to_offset(offset);
    let hour24 = localized.hour();
    let hour12 = match hour24 % 12 {
        0 => 12,
        value => value,
    };
    let meridiem = if hour24 < 12 { "am" } else { "pm" };
    let offset_seconds = offset.whole_seconds();
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    let total = offset_seconds.unsigned_abs();
    let tz_hours = total / 3600;
    let tz_minutes = (total % 3600) / 60;

    format!(
        "{}:{:02}{} GMT{}{:02}:{:02}",
        hour12,
        localized.minute(),
        meridiem,
        sign,
        tz_hours,
        tz_minutes
    )
}

pub(super) fn sum_json_value(value: &JsonValue) -> Result<JsonValue, JsonError> {
    match value {
        JsonValue::Undefined | JsonValue::Null => Ok(JsonValue::Undefined),
        JsonValue::Array(array) => {
            if array.elements.is_empty() {
                return Ok(JsonValue::Undefined);
            }

            let mut total = 0.0;
            for element in &array.elements {
                let Some(number) = json_value_to_number(element) else {
                    return Err(JsonError::new(
                        "D3050",
                        "$sum() expects the input array to contain only numeric values",
                    ));
                };
                total += number;
            }
            Ok(JsonValue::Number(total))
        }
        other => {
            let Some(number) = json_value_to_number(other) else {
                return Err(JsonError::new(
                    "D3050",
                    "$sum() expects a numeric argument or an array of numerics",
                ));
            };
            Ok(JsonValue::Number(number))
        }
    }
}

pub(super) fn clone_json_value(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Array(array) => JsonValue::Array(crate::types::JsonArray::new(
            array.elements.iter().map(clone_json_value).collect(),
            array.is_sequence,
            array.outer_wrapper,
        )),
        JsonValue::Object(JsonObject(entries)) => JsonValue::Object(JsonObject(
            entries
                .iter()
                .map(|(key, item)| (key.clone(), clone_json_value(item)))
                .collect(),
        )),
        other => other.clone(),
    }
}
