use crate::types::{JsonArray, JsonError, JsonObject, JsonValue};
use ryu::Buffer;
use serde::Serialize;
use serde_json::ser::{CompactFormatter, Formatter, PrettyFormatter, Serializer};
use serde_json::{Map, Number, Value};
use std::io;

struct JsonataFormatter<F> {
    inner: F,
}

impl<F> JsonataFormatter<F> {
    fn new(inner: F) -> Self {
        Self { inner }
    }
}

impl<F: Formatter> Formatter for JsonataFormatter<F> {
    fn write_null<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_null(writer)
    }

    fn write_bool<W>(&mut self, writer: &mut W, value: bool) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_bool(writer, value)
    }

    fn write_i8<W>(&mut self, writer: &mut W, value: i8) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_i8(writer, value)
    }

    fn write_i16<W>(&mut self, writer: &mut W, value: i16) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_i16(writer, value)
    }

    fn write_i32<W>(&mut self, writer: &mut W, value: i32) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_i32(writer, value)
    }

    fn write_i64<W>(&mut self, writer: &mut W, value: i64) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_i64(writer, value)
    }

    fn write_i128<W>(&mut self, writer: &mut W, value: i128) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_i128(writer, value)
    }

    fn write_u8<W>(&mut self, writer: &mut W, value: u8) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_u8(writer, value)
    }

    fn write_u16<W>(&mut self, writer: &mut W, value: u16) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_u16(writer, value)
    }

    fn write_u32<W>(&mut self, writer: &mut W, value: u32) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_u32(writer, value)
    }

    fn write_u64<W>(&mut self, writer: &mut W, value: u64) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_u64(writer, value)
    }

    fn write_u128<W>(&mut self, writer: &mut W, value: u128) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_u128(writer, value)
    }

    fn write_f32<W>(&mut self, writer: &mut W, value: f32) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_f32(writer, value)
    }

    fn write_f64<W>(&mut self, writer: &mut W, value: f64) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        write_js_number(writer, value)
    }

    fn write_number_str<W>(&mut self, writer: &mut W, value: &str) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_number_str(writer, value)
    }

    fn begin_string<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.begin_string(writer)
    }

    fn end_string<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.end_string(writer)
    }

    fn write_string_fragment<W>(&mut self, writer: &mut W, fragment: &str) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_string_fragment(writer, fragment)
    }

    fn write_char_escape<W>(
        &mut self,
        writer: &mut W,
        char_escape: serde_json::ser::CharEscape,
    ) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_char_escape(writer, char_escape)
    }

    fn write_byte_array<W>(&mut self, writer: &mut W, value: &[u8]) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_byte_array(writer, value)
    }

    fn begin_array<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.begin_array(writer)
    }

    fn end_array<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.end_array(writer)
    }

    fn begin_array_value<W>(&mut self, writer: &mut W, first: bool) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.begin_array_value(writer, first)
    }

    fn end_array_value<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.end_array_value(writer)
    }

    fn begin_object<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.begin_object(writer)
    }

    fn end_object<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.end_object(writer)
    }

    fn begin_object_key<W>(&mut self, writer: &mut W, first: bool) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.begin_object_key(writer, first)
    }

    fn end_object_key<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.end_object_key(writer)
    }

    fn begin_object_value<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.begin_object_value(writer)
    }

    fn end_object_value<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.end_object_value(writer)
    }
}

fn format_js_number(value: f64) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }

    if value.fract() == 0.0 && value.abs() < 1e21 {
        let formatted = format!("{:.0}", value);
        return if formatted == "-0" {
            "0".to_owned()
        } else {
            formatted
        };
    }

    let mut buffer = Buffer::new();
    let mut formatted = buffer.format_finite(value).to_owned();
    if let Some(pos) = formatted.find('e') {
        let exponent = &formatted[pos + 1..];
        if !exponent.starts_with('-') && !exponent.starts_with('+') {
            formatted.insert(pos + 1, '+');
        }
    }
    if formatted == "-0" {
        formatted = "0".to_owned();
    }
    formatted
}

fn write_js_number<W>(writer: &mut W, value: f64) -> io::Result<()>
where
    W: ?Sized + io::Write,
{
    let formatted = format_js_number(value);
    writer.write_all(formatted.as_bytes())
}

fn to_jsonata_value(value: &JsonValue) -> Result<Value, JsonError> {
    match value {
        JsonValue::Undefined => Ok(Value::Null),
        JsonValue::Null => Ok(Value::Null),
        JsonValue::Bool(flag) => Ok(Value::Bool(*flag)),
        JsonValue::Number(num) => {
            if !num.is_finite() {
                return Err(JsonError::new(
                    "D3001",
                    format!("Unable to represent number {}", num),
                ));
            }
            Ok(Number::from_f64(*num)
                .map(Value::Number)
                .unwrap_or(Value::Null))
        }
        JsonValue::String(text) => Ok(Value::String(text.clone())),
        JsonValue::Array(JsonArray { elements, .. }) => {
            let converted = elements
                .iter()
                .map(|element| {
                    if matches!(element, JsonValue::Undefined) {
                        Ok(Value::Null)
                    } else {
                        to_jsonata_value(element)
                    }
                })
                .collect::<Result<Vec<Value>, JsonError>>()?;
            Ok(Value::Array(converted))
        }
        JsonValue::Object(JsonObject(entries)) => {
            let mut map = Map::new();
            for (key, entry_value) in entries {
                if matches!(entry_value, JsonValue::Undefined) {
                    continue;
                }
                map.insert(key.clone(), to_jsonata_value(entry_value)?);
            }
            Ok(Value::Object(map))
        }
    }
}

fn to_number(value: &JsonValue) -> Option<f64> {
    match value {
        JsonValue::Undefined => None,
        JsonValue::Null => Some(0.0),
        JsonValue::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        JsonValue::Number(num) => Some(*num),
        JsonValue::String(text) => text.parse::<f64>().ok(),
        JsonValue::Array(_) | JsonValue::Object(_) => None,
    }
}

fn to_integer(value: &JsonValue) -> Option<i64> {
    to_number(value).map(|num| num.trunc() as i64)
}

fn ensure_string(value: &JsonValue, prettify: bool) -> Result<Option<String>, JsonError> {
    match value {
        JsonValue::Undefined => Ok(None),
        JsonValue::Null => Ok(Some("null".to_owned())),
        JsonValue::Bool(flag) => Ok(Some(flag.to_string())),
        JsonValue::Number(num) => {
            if !num.is_finite() {
                Err(JsonError::new(
                    "D3001",
                    format!("Unable to represent number {}", num),
                ))
            } else {
                Ok(Some(num.to_string()))
            }
        }
        JsonValue::String(text) => Ok(Some(text.clone())),
        JsonValue::Array(JsonArray {
            elements,
            outer_wrapper,
            ..
        }) if *outer_wrapper => {
            if let Some(first) = elements.first() {
                ensure_string(first, prettify)
            } else {
                Ok(None)
            }
        }
        _ => match string(value, prettify)? {
            JsonValue::Undefined => Ok(None),
            JsonValue::String(text) => Ok(Some(text)),
            _ => Err(JsonError::new("D3137", "Unable to convert value to string")),
        },
    }
}

fn slice_chars(chars: &[char], start: i64, length: Option<i64>) -> String {
    let len = chars.len() as i64;
    let mut start_idx = start;
    if len + start_idx < 0 {
        start_idx = 0;
    }

    let take = match length {
        Some(len_arg) if len_arg <= 0 => return String::new(),
        Some(len_arg) => {
            if start_idx >= 0 {
                (start_idx + len_arg).min(len)
            } else {
                (len + start_idx + len_arg).min(len)
            }
        }
        None => len,
    };

    let start_resolved = if start_idx >= 0 {
        start_idx.min(len)
    } else {
        (len + start_idx).max(0)
    };

    let end_resolved = take.max(start_resolved).min(len);

    chars[start_resolved as usize..end_resolved as usize]
        .iter()
        .collect::<String>()
}

pub fn string(value: &JsonValue, prettify: bool) -> Result<JsonValue, JsonError> {
    match value {
        JsonValue::Undefined => return Ok(JsonValue::Undefined),
        JsonValue::String(text) => return Ok(JsonValue::String(text.clone())),
        JsonValue::Number(num) => {
            if !num.is_finite() {
                return Err(JsonError::new(
                    "D3001",
                    format!("Unable to represent number {}", num),
                ));
            }
        }
        _ => {}
    }

    let mut target = value;
    if let JsonValue::Array(JsonArray {
        elements,
        outer_wrapper,
        ..
    }) = value
    {
        if *outer_wrapper {
            if let Some(first) = elements.first() {
                target = first;
            } else {
                return Ok(JsonValue::Undefined);
            }
        }
    }

    match target {
        JsonValue::Undefined => Ok(JsonValue::Undefined),
        JsonValue::Null => Ok(JsonValue::String("null".to_owned())),
        JsonValue::Bool(flag) => Ok(JsonValue::String(flag.to_string())),
        JsonValue::Number(num) => Ok(JsonValue::String(format_js_number(*num))),
        JsonValue::String(text) => Ok(JsonValue::String(text.clone())),
        JsonValue::Array(_) | JsonValue::Object(_) => {
            let serde_value = to_jsonata_value(target)?;
            let mut buffer = Vec::new();
            if prettify {
                let formatter = JsonataFormatter::new(PrettyFormatter::with_indent(b"  "));
                let mut serializer = Serializer::with_formatter(&mut buffer, formatter);
                serde_value
                    .serialize(&mut serializer)
                    .map_err(|err| JsonError::new("D3137", err.to_string()))?;
            } else {
                let formatter = JsonataFormatter::new(CompactFormatter {});
                let mut serializer = Serializer::with_formatter(&mut buffer, formatter);
                serde_value
                    .serialize(&mut serializer)
                    .map_err(|err| JsonError::new("D3137", err.to_string()))?;
            }
            let result = String::from_utf8(buffer)
                .map_err(|err| JsonError::new("D3137", err.to_string()))?;
            Ok(JsonValue::String(result))
        }
    }
}

pub fn substring(
    value: &JsonValue,
    start: &JsonValue,
    length: &JsonValue,
) -> Result<JsonValue, JsonError> {
    let string_value = match ensure_string(value, false)? {
        Some(str_val) => str_val,
        None => return Ok(JsonValue::Undefined),
    };

    let chars: Vec<char> = string_value.chars().collect();
    let length_idx = to_integer(length);
    let mut start_idx = to_integer(start).unwrap_or(0);
    if chars.len() as i64 + start_idx < 0 {
        start_idx = 0;
    }
    let result = slice_chars(&chars, start_idx, length_idx);
    Ok(JsonValue::String(result))
}

pub fn substring_before(value: &JsonValue, chars: &JsonValue) -> Result<JsonValue, JsonError> {
    let string_value = match ensure_string(value, false)? {
        Some(str_val) => str_val,
        None => return Ok(JsonValue::Undefined),
    };
    let chars_value = ensure_string(chars, false)?.unwrap_or_else(|| "undefined".to_owned());

    if let Some(pos) = string_value.find(&chars_value) {
        Ok(JsonValue::String(string_value[..pos].to_owned()))
    } else {
        Ok(JsonValue::String(string_value))
    }
}

pub fn substring_after(value: &JsonValue, chars: &JsonValue) -> Result<JsonValue, JsonError> {
    let string_value = match ensure_string(value, false)? {
        Some(str_val) => str_val,
        None => return Ok(JsonValue::Undefined),
    };
    let chars_value = ensure_string(chars, false)?.unwrap_or_else(|| "undefined".to_owned());

    if let Some(pos) = string_value.find(&chars_value) {
        Ok(JsonValue::String(
            string_value[pos + chars_value.len()..].to_owned(),
        ))
    } else {
        Ok(JsonValue::String(string_value))
    }
}

pub fn lowercase(value: &JsonValue) -> Result<JsonValue, JsonError> {
    let string_value = match ensure_string(value, false)? {
        Some(str_val) => str_val,
        None => return Ok(JsonValue::Undefined),
    };
    Ok(JsonValue::String(string_value.to_lowercase()))
}

pub fn uppercase(value: &JsonValue) -> Result<JsonValue, JsonError> {
    let string_value = match ensure_string(value, false)? {
        Some(str_val) => str_val,
        None => return Ok(JsonValue::Undefined),
    };
    Ok(JsonValue::String(string_value.to_uppercase()))
}

pub fn length(value: &JsonValue) -> Result<JsonValue, JsonError> {
    let string_value = match ensure_string(value, false)? {
        Some(str_val) => str_val,
        None => return Ok(JsonValue::Undefined),
    };
    let count = string_value.chars().count() as f64;
    Ok(JsonValue::Number(count))
}

pub fn trim(value: &JsonValue) -> Result<JsonValue, JsonError> {
    let string_value = match ensure_string(value, false)? {
        Some(str_val) => str_val,
        None => return Ok(JsonValue::Undefined),
    };
    let normalized = string_value
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ");
    Ok(JsonValue::String(normalized))
}

pub fn pad(
    value: &JsonValue,
    width: &JsonValue,
    char_value: &JsonValue,
) -> Result<JsonValue, JsonError> {
    let string_value = match ensure_string(value, false)? {
        Some(str_val) => str_val,
        None => return Ok(JsonValue::Undefined),
    };

    let width_num = to_integer(width).unwrap_or(0);
    if width_num == 0 {
        return Ok(JsonValue::String(string_value));
    }

    let pad_char = ensure_string(char_value, false)?
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| " ".to_owned());

    let current_len = string_value.chars().count();
    let target_len = width_num.abs() as usize;
    if target_len <= current_len {
        return Ok(JsonValue::String(string_value));
    }

    let pad_length = target_len - current_len;
    let mut padding = String::new();
    while padding.chars().count() < pad_length {
        padding.push_str(&pad_char);
    }
    let padding: String = padding.chars().take(pad_length).collect();

    let result = if width_num > 0 {
        format!("{}{}", string_value, padding)
    } else {
        format!("{}{}", padding, string_value)
    };

    Ok(JsonValue::String(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_undefined() {
        assert!(matches!(
            string(&JsonValue::Undefined, false).unwrap(),
            JsonValue::Undefined
        ));
    }

    #[test]
    fn string_passthrough() {
        let value = JsonValue::String("hello".to_owned());
        let result = string(&value, false).unwrap();
        assert!(matches!(result, JsonValue::String(ref s) if s == "hello"));
    }

    #[test]
    fn string_number() {
        let value = JsonValue::Number(42.0);
        let result = string(&value, false).unwrap();
        assert!(matches!(result, JsonValue::String(ref s) if s == "42"));
    }

    #[test]
    fn string_array_of_numbers_matches_json() {
        let array = JsonValue::Array(JsonArray::new(
            vec![JsonValue::Number(1.0), JsonValue::Number(2.0)],
            false,
            false,
        ));
        let result = string(&array, false).unwrap();
        assert!(matches!(result, JsonValue::String(ref s) if s == "[1,2]"));
    }

    #[test]
    fn string_single_element_array_preserves_brackets() {
        let array = JsonValue::Array(JsonArray::new(
            vec![JsonValue::Number(2.0)],
            false,
            false,
        ));
        let result = string(&array, false).unwrap();
        assert!(matches!(result, JsonValue::String(ref s) if s == "[2]"));
    }

    #[test]
    fn string_large_number_uses_exponent_plus() {
        let value = JsonValue::Number(1e21);
        let result = string(&value, false).unwrap();
        assert!(matches!(result, JsonValue::String(ref s) if s == "1e+21"));
    }

    #[test]
    fn substring_basic() {
        let value = JsonValue::String("Hello".to_owned());
        let start = JsonValue::Number(1.0);
        let length = JsonValue::Number(2.0);
        let result = substring(&value, &start, &length).unwrap();
        assert!(matches!(result, JsonValue::String(ref s) if s == "el"));
    }

    #[test]
    fn pad_left() {
        let value = JsonValue::String("7".to_owned());
        let width = JsonValue::Number(-3.0);
        let pad_char = JsonValue::String("0".to_owned());
        let result = pad(&value, &width, &pad_char).unwrap();
        assert!(matches!(result, JsonValue::String(ref s) if s == "007"));
    }
}
