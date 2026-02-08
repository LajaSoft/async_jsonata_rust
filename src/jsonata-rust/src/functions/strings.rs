use crate::functions::math::normalize_js_number;
use crate::types::{JsonArray, JsonError, JsonObject, JsonValue};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use ryu::Buffer;
use serde::Serialize;
use serde_json::ser::{CompactFormatter, Formatter, PrettyFormatter, Serializer};
use serde_json::{Map, Number, Value};
use std::io;

const ENCODE_URI_COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

const ENCODE_URI: &AsciiSet = &ENCODE_URI_COMPONENT
    .remove(b';')
    .remove(b'/')
    .remove(b'?')
    .remove(b':')
    .remove(b'@')
    .remove(b'&')
    .remove(b'=')
    .remove(b'+')
    .remove(b'$')
    .remove(b',')
    .remove(b'#');

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
    fn round_to_precision_15(input: f64) -> f64 {
        let scientific = format!("{:.14e}", input);
        scientific.parse::<f64>().unwrap_or(input)
    }

    fn scientific_to_decimal(scientific: &str) -> Option<String> {
        let lower = scientific.to_ascii_lowercase();
        let (mantissa, exponent_str) = lower.split_once('e')?;
        let exponent = exponent_str.parse::<i32>().ok()?;

        let negative = mantissa.starts_with('-');
        let digits_only: String = mantissa
            .trim_start_matches('-')
            .chars()
            .filter(|c| *c != '.')
            .collect();
        let decimal_pos = mantissa
            .trim_start_matches('-')
            .find('.')
            .unwrap_or(mantissa.trim_start_matches('-').len()) as i32;
        let new_pos = decimal_pos + exponent;

        let mut out = if new_pos <= 0 {
            format!("0.{}{}", "0".repeat((-new_pos) as usize), digits_only)
        } else if new_pos as usize >= digits_only.len() {
            format!("{}{}", digits_only, "0".repeat(new_pos as usize - digits_only.len()))
        } else {
            let idx = new_pos as usize;
            format!("{}.{}", &digits_only[..idx], &digits_only[idx..])
        };

        while out.ends_with('0') && out.contains('.') {
            out.pop();
        }
        if out.ends_with('.') {
            out.pop();
        }
        if negative && out != "0" {
            out.insert(0, '-');
        }
        Some(out)
    }

    let value = round_to_precision_15(normalize_js_number(value));
    if value == 0.0 {
        return "0".to_owned();
    }

    let abs = value.abs();
    if abs >= 1e-6 && abs < 1e21 {
        let mut buffer = Buffer::new();
        let rendered = buffer.format_finite(value).to_owned();
        let mut fixed = if rendered.contains('e') || rendered.contains('E') {
            scientific_to_decimal(&rendered).unwrap_or(rendered)
        } else {
            rendered
        };
        if fixed.contains('.') {
            while fixed.ends_with('0') {
                fixed.pop();
            }
            if fixed.ends_with('.') {
                fixed.pop();
            }
        }
        if fixed == "-0" {
            return "0".to_owned();
        }
        return fixed;
    }

    let scientific = format!("{:.14e}", value);
    let mut parts = scientific.split('e');
    let mut mantissa = parts.next().unwrap_or("0").to_owned();
    while mantissa.ends_with('0') {
        mantissa.pop();
    }
    if mantissa.ends_with('.') {
        mantissa.pop();
    }
    let exponent = parts
        .next()
        .and_then(|exp| exp.parse::<i32>().ok())
        .unwrap_or(0);

    let formatted = format!("{}e{:+}", mantissa, exponent);
    if formatted == "-0e+0" {
        "0".to_owned()
    } else {
        formatted
    }
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
                    "D1001",
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
            let is_function_object = entries.iter().any(|(key, entry_value)| {
                (key == "_jsonata_function" || key == "_jsonata_lambda")
                    && matches!(entry_value, JsonValue::Bool(true))
            });
            if is_function_object {
                return Ok(Value::String(String::new()));
            }
            let mut map = Map::new();
            for (key, entry_value) in entries {
                if matches!(entry_value, JsonValue::Undefined) {
                    continue;
                }
                map.insert(key.clone(), to_jsonata_value(entry_value)?);
            }
            Ok(Value::Object(map))
        }
        JsonValue::Function(_) => Ok(Value::String(String::new())),
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
        JsonValue::Function(_) => None,
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
                Ok(Some(format_js_number(*num)))
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
        JsonValue::Function(_) => Err(JsonError::new(
            "D3137",
            "Unable to convert function to string",
        )),
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
        JsonValue::Function(_) => {
            return Ok(JsonValue::String(String::new()));
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
        JsonValue::Function(_) => Ok(JsonValue::String(String::new())),
    }
}

fn coerce_to_string(value: &JsonValue) -> Result<String, JsonError> {
    match value {
        JsonValue::String(text) => Ok(text.clone()),
        _ => match string(value, false)? {
            JsonValue::String(text) => Ok(text),
            JsonValue::Undefined => Ok(String::new()),
            _ => Ok(String::new()),
        },
    }
}

fn strict_percent_decode(input: &str) -> Result<Vec<u8>, ()> {
    let mut buffer: Vec<u8> = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' {
            if index + 2 >= bytes.len() {
                return Err(());
            }
            let high = hex_value(bytes[index + 1])?;
            let low = hex_value(bytes[index + 2])?;
            buffer.push((high << 4) | low);
            index += 3;
        } else {
            buffer.push(byte);
            index += 1;
        }
    }
    Ok(buffer)
}

fn hex_value(byte: u8) -> Result<u8, ()> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(()),
    }
}

pub fn base64encode(value: &JsonValue) -> Result<JsonValue, JsonError> {
    if matches!(value, JsonValue::Undefined) {
        return Ok(JsonValue::Undefined);
    }
    let text = coerce_to_string(value)?;
    let encoded = BASE64_STANDARD.encode(text.as_bytes());
    Ok(JsonValue::String(encoded))
}

pub fn base64decode(value: &JsonValue) -> Result<JsonValue, JsonError> {
    if matches!(value, JsonValue::Undefined) {
        return Ok(JsonValue::Undefined);
    }
    let text = coerce_to_string(value)?;
    let decoded = BASE64_STANDARD
        .decode(text.as_bytes())
        .map_err(|err| JsonError::new("D3140", format!("Invalid base64 input: {}", err)))?;
    let output: String = decoded.into_iter().map(char::from).collect();
    Ok(JsonValue::String(output))
}

pub fn encode_url_component(value: &JsonValue) -> Result<JsonValue, JsonError> {
    if matches!(value, JsonValue::Undefined) {
        return Ok(JsonValue::Undefined);
    }
    let text = coerce_to_string(value)?;
    if text.contains('\u{FFFD}') {
        return Err(JsonError::new(
            "D3140",
            format!("Malformed URL passed to $encodeUrlComponent(): {:?}", text),
        ));
    }
    let encoded = utf8_percent_encode(&text, ENCODE_URI_COMPONENT).to_string();
    Ok(JsonValue::String(encoded))
}

pub fn encode_url(value: &JsonValue) -> Result<JsonValue, JsonError> {
    if matches!(value, JsonValue::Undefined) {
        return Ok(JsonValue::Undefined);
    }
    let text = coerce_to_string(value)?;
    if text.contains('\u{FFFD}') {
        return Err(JsonError::new(
            "D3140",
            format!("Malformed URL passed to $encodeUrl(): {:?}", text),
        ));
    }
    let encoded = utf8_percent_encode(&text, ENCODE_URI).to_string();
    Ok(JsonValue::String(encoded))
}

fn decode_uri_value(value: &JsonValue, function_name: &str) -> Result<JsonValue, JsonError> {
    if matches!(value, JsonValue::Undefined) {
        return Ok(JsonValue::Undefined);
    }
    let text = coerce_to_string(value)?;
    let decoded_bytes = strict_percent_decode(&text).map_err(|_| {
        JsonError::new(
            "D3140",
            format!("Malformed URL passed to {}(): {:?}", function_name, text),
        )
    })?;
    let decoded_string = String::from_utf8(decoded_bytes).map_err(|_| {
        JsonError::new(
            "D3140",
            format!("Malformed URL passed to {}(): {:?}", function_name, text),
        )
    })?;
    Ok(JsonValue::String(decoded_string))
}

pub fn decode_url_component(value: &JsonValue) -> Result<JsonValue, JsonError> {
    decode_uri_value(value, "$decodeUrlComponent")
}

pub fn decode_url(value: &JsonValue) -> Result<JsonValue, JsonError> {
    decode_uri_value(value, "$decodeUrl")
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

#[derive(Clone)]
struct NumberFormatProperties {
    decimal_separator: String,
    grouping_separator: String,
    exponent_separator: String,
    minus_sign: String,
    percent: String,
    per_mille: String,
    zero_digit: String,
    digit: String,
    pattern_separator: String,
}

#[derive(Clone)]
struct NumberFormatParts {
    prefix: String,
    suffix: String,
    active_part: String,
    mantissa_part: String,
    exponent_part: Option<String>,
    integer_part: String,
    fractional_part: String,
    subpicture: String,
}

#[derive(Clone)]
struct NumberFormatPicture {
    integer_part_grouping_positions: Vec<usize>,
    regular_grouping: usize,
    minimum_integer_part_size: usize,
    scaling_factor: usize,
    fractional_part_grouping_positions: Vec<usize>,
    minimum_fractional_part_size: usize,
    maximum_fractional_part_size: usize,
    minimum_exponent_size: usize,
    prefix: String,
    suffix: String,
    picture: String,
}

fn repeat_token(token: &str, count: usize) -> String {
    if count == 0 {
        return String::new();
    }
    token.repeat(count)
}

fn first_char_or(input: &str, fallback: char) -> char {
    input.chars().next().unwrap_or(fallback)
}

fn count_occurrences(input: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    input.match_indices(needle).count()
}

fn count_decimal_digits(input: &str, decimal_digit_family: &[char]) -> usize {
    input.chars()
        .filter(|ch| decimal_digit_family.contains(ch))
        .count()
}

fn count_decimal_or_optional_digits(
    input: &str,
    decimal_digit_family: &[char],
    digit_placeholder: char,
) -> usize {
    input.chars()
        .filter(|ch| decimal_digit_family.contains(ch) || *ch == digit_placeholder)
        .count()
}

fn split_parts(
    subpicture: &str,
    props: &NumberFormatProperties,
    active_chars: &[char],
) -> NumberFormatParts {
    let exponent_separator = first_char_or(&props.exponent_separator, 'e');
    let mut prefix = String::new();
    let mut hit_active = false;
    for ch in subpicture.chars() {
        if active_chars.contains(&ch) && ch != exponent_separator {
            hit_active = true;
            break;
        }
        prefix.push(ch);
    }
    if !hit_active {
        prefix.clear();
    }

    let mut suffix_reversed = String::new();
    let mut hit_active_from_right = false;
    for ch in subpicture.chars().rev() {
        if active_chars.contains(&ch) && ch != exponent_separator {
            hit_active_from_right = true;
            break;
        }
        suffix_reversed.push(ch);
    }
    let suffix: String = if hit_active_from_right {
        suffix_reversed.chars().rev().collect()
    } else {
        String::new()
    };

    let active_part = subpicture
        .strip_prefix(&prefix)
        .unwrap_or(subpicture)
        .strip_suffix(&suffix)
        .unwrap_or(subpicture)
        .to_owned();

    let exponent_position = subpicture
        .char_indices()
        .find(|(idx, ch)| *idx >= prefix.len() && *ch == exponent_separator)
        .map(|(idx, _)| idx)
        .filter(|idx| *idx <= subpicture.len().saturating_sub(suffix.len()));

    let (mantissa_part, exponent_part) = if let Some(position) = exponent_position {
        let left = active_part[..position - prefix.len()].to_owned();
        let right = active_part[position - prefix.len() + props.exponent_separator.len()..].to_owned();
        (left, Some(right))
    } else {
        (active_part.clone(), None)
    };

    let decimal_separator = first_char_or(&props.decimal_separator, '.');
    let decimal_position = mantissa_part.find(decimal_separator);
    let (integer_part, fractional_part) = if let Some(position) = decimal_position {
        (
            mantissa_part[..position].to_owned(),
            mantissa_part[position + props.decimal_separator.len()..].to_owned(),
        )
    } else {
        (mantissa_part.clone(), String::new())
    };

    NumberFormatParts {
        prefix,
        suffix,
        active_part,
        mantissa_part,
        exponent_part,
        integer_part,
        fractional_part,
        subpicture: subpicture.to_owned(),
    }
}

fn validate_picture(
    parts: &NumberFormatParts,
    props: &NumberFormatProperties,
    decimal_digit_family: &[char],
    active_chars: &[char],
) -> Result<(), JsonError> {
    let decimal_separator = first_char_or(&props.decimal_separator, '.');
    let grouping_separator = first_char_or(&props.grouping_separator, ',');
    let digit_placeholder = first_char_or(&props.digit, '#');

    let mut error_code: Option<&'static str> = None;
    let decimal_pos = parts.subpicture.find(decimal_separator);
    if decimal_pos != parts.subpicture.rfind(decimal_separator) {
        error_code = Some("D3081");
    }
    if count_occurrences(&parts.subpicture, &props.percent) > 1 {
        error_code = Some("D3082");
    }
    if count_occurrences(&parts.subpicture, &props.per_mille) > 1 {
        error_code = Some("D3083");
    }
    if parts.subpicture.contains(&props.percent) && parts.subpicture.contains(&props.per_mille) {
        error_code = Some("D3084");
    }

    let has_active_digit = parts
        .mantissa_part
        .chars()
        .any(|ch| decimal_digit_family.contains(&ch) || ch == digit_placeholder);
    if !has_active_digit {
        error_code = Some("D3085");
    }

    let has_passive_char = parts.active_part.chars().any(|ch| !active_chars.contains(&ch));
    if has_passive_char {
        error_code = Some("D3086");
    }

    if let Some(decimal_position) = decimal_pos {
        let chars: Vec<char> = parts.subpicture.chars().collect();
        let decimal_char_index = parts.subpicture[..decimal_position].chars().count();
        let before = if decimal_char_index > 0 {
            Some(chars[decimal_char_index - 1])
        } else {
            None
        };
        let after = chars.get(decimal_char_index + 1).copied();
        if before == Some(grouping_separator) || after == Some(grouping_separator) {
            error_code = Some("D3087");
        }
    } else if parts.integer_part.ends_with(grouping_separator) {
        error_code = Some("D3088");
    }

    if parts.subpicture.contains(&(props.grouping_separator.clone() + &props.grouping_separator)) {
        error_code = Some("D3089");
    }

    if let Some(optional_digit_pos) = parts.integer_part.find(digit_placeholder) {
        let has_decimal_before_optional = parts.integer_part[..optional_digit_pos]
            .chars()
            .any(|ch| decimal_digit_family.contains(&ch));
        if has_decimal_before_optional {
            error_code = Some("D3090");
        }
    }

    if let Some(optional_digit_pos) = parts.fractional_part.rfind(digit_placeholder) {
        let has_decimal_after_optional = parts.fractional_part[optional_digit_pos..]
            .chars()
            .any(|ch| decimal_digit_family.contains(&ch));
        if has_decimal_after_optional {
            error_code = Some("D3091");
        }
    }

    if let Some(exponent_part) = &parts.exponent_part {
        if !exponent_part.is_empty()
            && (parts.subpicture.contains(&props.percent) || parts.subpicture.contains(&props.per_mille))
        {
            error_code = Some("D3092");
        }
        if exponent_part.is_empty()
            || exponent_part
                .chars()
                .any(|ch| !decimal_digit_family.contains(&ch))
        {
            error_code = Some("D3093");
        }
    }

    if let Some(code) = error_code {
        return Err(JsonError::new(code, "Invalid picture string"));
    }
    Ok(())
}

fn analyse_picture(
    parts: &NumberFormatParts,
    props: &NumberFormatProperties,
    decimal_digit_family: &[char],
) -> NumberFormatPicture {
    let grouping_separator = first_char_or(&props.grouping_separator, ',');
    let digit_placeholder = first_char_or(&props.digit, '#');

    let get_grouping_positions = |part: &str, to_left: bool| -> Vec<usize> {
        let mut positions: Vec<usize> = Vec::new();
        let mut search_start = 0usize;
        while let Some(offset) = part[search_start..].find(grouping_separator) {
            let grouping_position = search_start + offset;
            let sample = if to_left {
                &part[..grouping_position]
            } else {
                &part[grouping_position..]
            };
            let chars_to_the_right = sample
                .chars()
                .filter(|ch| decimal_digit_family.contains(ch) || *ch == digit_placeholder)
                .count();
            positions.push(chars_to_the_right);
            search_start = grouping_position + grouping_separator.len_utf8();
        }
        positions
    };

    let integer_part_grouping_positions = get_grouping_positions(&parts.integer_part, false);
    let regular_grouping = if integer_part_grouping_positions.is_empty() {
        0
    } else {
        let gcd = |mut a: usize, mut b: usize| -> usize {
            while b != 0 {
                let t = b;
                b = a % b;
                a = t;
            }
            a
        };
        let factor = integer_part_grouping_positions
            .iter()
            .copied()
            .reduce(gcd)
            .unwrap_or(0);
        let mut index = 1usize;
        let mut regular = true;
        while index <= integer_part_grouping_positions.len() {
            if !integer_part_grouping_positions.contains(&(index * factor)) {
                regular = false;
                break;
            }
            index += 1;
        }
        if regular {
            factor
        } else {
            0
        }
    };

    let fractional_part_grouping_positions = get_grouping_positions(&parts.fractional_part, true);
    let mut minimum_integer_part_size = count_decimal_digits(&parts.integer_part, decimal_digit_family);
    let scaling_factor = minimum_integer_part_size;
    let mut minimum_fractional_part_size =
        count_decimal_digits(&parts.fractional_part, decimal_digit_family);
    let mut maximum_fractional_part_size = count_decimal_or_optional_digits(
        &parts.fractional_part,
        decimal_digit_family,
        digit_placeholder,
    );
    let exponent_present = parts.exponent_part.is_some();

    if minimum_integer_part_size == 0 && maximum_fractional_part_size == 0 {
        if exponent_present {
            minimum_fractional_part_size = 1;
            maximum_fractional_part_size = 1;
        } else {
            minimum_integer_part_size = 1;
        }
    }

    if exponent_present
        && minimum_integer_part_size == 0
        && parts.integer_part.contains(digit_placeholder)
    {
        minimum_integer_part_size = 1;
    }

    if minimum_integer_part_size == 0 && minimum_fractional_part_size == 0 {
        minimum_fractional_part_size = 1;
    }

    let minimum_exponent_size = if let Some(exponent_part) = &parts.exponent_part {
        count_decimal_digits(exponent_part, decimal_digit_family)
    } else {
        0
    };

    NumberFormatPicture {
        integer_part_grouping_positions,
        regular_grouping,
        minimum_integer_part_size,
        scaling_factor,
        fractional_part_grouping_positions,
        minimum_fractional_part_size,
        maximum_fractional_part_size,
        minimum_exponent_size,
        prefix: parts.prefix.clone(),
        suffix: parts.suffix.clone(),
        picture: parts.subpicture.clone(),
    }
}

fn insert_char_at(input: &str, char_index: usize, ch: char) -> String {
    let mut chars: Vec<char> = input.chars().collect();
    chars.insert(char_index, ch);
    chars.into_iter().collect()
}

fn to_fixed_abs(value: f64, dp: usize) -> String {
    format!("{:.*}", dp, value.abs())
}

fn to_radix_string(mut value: i128, radix: u32) -> String {
    if value == 0 {
        return "0".to_owned();
    }

    let negative = value < 0;
    if negative {
        value = -value;
    }

    let mut digits: Vec<char> = Vec::new();
    while value > 0 {
        let digit = (value % radix as i128) as u32;
        let ch = if digit < 10 {
            char::from_u32('0' as u32 + digit).unwrap_or('0')
        } else {
            char::from_u32('a' as u32 + (digit - 10)).unwrap_or('a')
        };
        digits.push(ch);
        value /= radix as i128;
    }

    digits.reverse();
    let mut out: String = digits.into_iter().collect();
    if negative {
        out.insert(0, '-');
    }
    out
}

pub fn format_base(value: &JsonValue, radix: &JsonValue) -> Result<JsonValue, JsonError> {
    let Some(raw_value) = to_number(value) else {
        return Ok(JsonValue::Undefined);
    };

    let rounded_value = crate::functions::math::round(Some(raw_value), None).unwrap_or(raw_value);
    let rounded_radix = match to_number(radix) {
        Some(base) => crate::functions::math::round(Some(base), None).unwrap_or(base),
        None => 10.0,
    };

    if !(2.0..=36.0).contains(&rounded_radix) {
        return Err(JsonError::new(
            "D3100",
            format!("Radix {} is out of range", rounded_radix),
        ));
    }

    let result = to_radix_string(rounded_value as i128, rounded_radix as u32);
    Ok(JsonValue::String(result))
}

pub fn format_number(
    value: &JsonValue,
    picture: &JsonValue,
    options: &JsonValue,
) -> Result<JsonValue, JsonError> {
    let Some(raw_value) = to_number(value) else {
        return Ok(JsonValue::Undefined);
    };

    let Some(picture_string) = ensure_string(picture, false)? else {
        return Ok(JsonValue::Undefined);
    };

    let mut props = NumberFormatProperties {
        decimal_separator: ".".to_owned(),
        grouping_separator: ",".to_owned(),
        exponent_separator: "e".to_owned(),
        minus_sign: "-".to_owned(),
        percent: "%".to_owned(),
        per_mille: "\u{2030}".to_owned(),
        zero_digit: "0".to_owned(),
        digit: "#".to_owned(),
        pattern_separator: ";".to_owned(),
    };

    if let JsonValue::Object(JsonObject(entries)) = options {
        for (key, entry_value) in entries {
            if let JsonValue::String(text) = entry_value {
                match key.as_str() {
                    "decimal-separator" => props.decimal_separator = text.clone(),
                    "grouping-separator" => props.grouping_separator = text.clone(),
                    "exponent-separator" => props.exponent_separator = text.clone(),
                    "minus-sign" => props.minus_sign = text.clone(),
                    "percent" => props.percent = text.clone(),
                    "per-mille" => props.per_mille = text.clone(),
                    "zero-digit" => props.zero_digit = text.clone(),
                    "digit" => props.digit = text.clone(),
                    "pattern-separator" => props.pattern_separator = text.clone(),
                    _ => {}
                }
            }
        }
    }

    let mut decimal_digit_family: Vec<char> = Vec::new();
    let zero_char_code = first_char_or(&props.zero_digit, '0') as u32;
    let mut codepoint = zero_char_code;
    while codepoint < zero_char_code + 10 {
        decimal_digit_family.push(char::from_u32(codepoint).unwrap_or('0'));
        codepoint += 1;
    }

    let digit_placeholder = first_char_or(&props.digit, '#');
    let mut active_chars = decimal_digit_family.clone();
    active_chars.push(first_char_or(&props.decimal_separator, '.'));
    active_chars.push(first_char_or(&props.exponent_separator, 'e'));
    active_chars.push(first_char_or(&props.grouping_separator, ','));
    active_chars.push(digit_placeholder);
    active_chars.push(first_char_or(&props.pattern_separator, ';'));

    let subpictures: Vec<&str> = picture_string.split(&props.pattern_separator).collect();
    if subpictures.len() > 2 {
        return Err(JsonError::new("D3080", "Too many subpictures in picture string"));
    }

    let mut pictures: Vec<NumberFormatPicture> = Vec::new();
    for subpicture in subpictures {
        let parts = split_parts(subpicture, &props, &active_chars);
        validate_picture(&parts, &props, &decimal_digit_family, &active_chars)?;
        pictures.push(analyse_picture(&parts, &props, &decimal_digit_family));
    }

    let decimal_separator = first_char_or(&props.decimal_separator, '.');
    let grouping_separator = first_char_or(&props.grouping_separator, ',');
    let zero_digit = first_char_or(&props.zero_digit, '0');

    if pictures.len() == 1 {
        let mut negative_pic = pictures[0].clone();
        negative_pic.prefix = format!("{}{}", props.minus_sign, negative_pic.prefix);
        pictures.push(negative_pic);
    }

    let pic = if raw_value >= 0.0 {
        &pictures[0]
    } else {
        &pictures[1]
    };

    let adjusted_number = if pic.picture.contains(&props.percent) {
        raw_value * 100.0
    } else if pic.picture.contains(&props.per_mille) {
        raw_value * 1000.0
    } else {
        raw_value
    };

    let mut mantissa = adjusted_number;
    let mut exponent: Option<i64> = None;
    if pic.minimum_exponent_size > 0 {
        let max_mantissa = 10f64.powi(pic.scaling_factor as i32);
        let min_mantissa = 10f64.powi(pic.scaling_factor.saturating_sub(1) as i32);
        let mut exp = 0i64;

        if mantissa != 0.0 {
            while mantissa.abs() < min_mantissa {
                mantissa *= 10.0;
                exp -= 1;
            }
            while mantissa.abs() > max_mantissa {
                mantissa /= 10.0;
                exp += 1;
            }
        }
        exponent = Some(exp);
    }

    let rounded_number = crate::functions::math::round(
        Some(mantissa),
        Some(pic.maximum_fractional_part_size as f64),
    )
    .unwrap_or(mantissa);

    let make_string = |num: f64, dp: usize| -> String {
        let mut text = to_fixed_abs(num, dp);
        if zero_digit == '0' {
            return text;
        }

        text = text
            .chars()
            .map(|digit| {
                if digit.is_ascii_digit() {
                    let index = digit as usize - '0' as usize;
                    return *decimal_digit_family.get(index).unwrap_or(&digit);
                }
                digit
            })
            .collect::<String>();
        text
    };

    let mut string_value = make_string(rounded_number, pic.maximum_fractional_part_size);
    if !string_value.contains('.') {
        string_value.push(decimal_separator);
    } else {
        string_value = string_value.replacen('.', &props.decimal_separator, 1);
    }

    while string_value.starts_with(zero_digit) {
        string_value = string_value.chars().skip(1).collect();
    }
    while string_value.ends_with(zero_digit) {
        string_value.pop();
    }

    let mut decimal_pos = string_value
        .find(decimal_separator)
        .map(|idx| string_value[..idx].chars().count())
        .unwrap_or(string_value.chars().count());

    let pad_left = pic.minimum_integer_part_size.saturating_sub(decimal_pos);
    let current_right = string_value.chars().count().saturating_sub(decimal_pos + 1);
    let pad_right = pic.minimum_fractional_part_size.saturating_sub(current_right);
    string_value = format!("{}{}", repeat_token(&props.zero_digit, pad_left), string_value);
    string_value = format!("{}{}", string_value, repeat_token(&props.zero_digit, pad_right));

    decimal_pos = string_value
        .find(decimal_separator)
        .map(|idx| string_value[..idx].chars().count())
        .unwrap_or(string_value.chars().count());

    if pic.regular_grouping > 0 {
        let group_count = decimal_pos.saturating_sub(1) / pic.regular_grouping;
        let mut group = 1usize;
        while group <= group_count {
            let insert_pos = decimal_pos - group * pic.regular_grouping;
            string_value = insert_char_at(&string_value, insert_pos, grouping_separator);
            group += 1;
        }
    } else {
        for pos in &pic.integer_part_grouping_positions {
            let insert_pos = decimal_pos.saturating_sub(*pos);
            string_value = insert_char_at(&string_value, insert_pos, grouping_separator);
            decimal_pos += 1;
        }
    }

    decimal_pos = string_value
        .find(decimal_separator)
        .map(|idx| string_value[..idx].chars().count())
        .unwrap_or(string_value.chars().count());
    for pos in &pic.fractional_part_grouping_positions {
        let insert_pos = pos + decimal_pos + 1;
        string_value = insert_char_at(&string_value, insert_pos, grouping_separator);
    }

    decimal_pos = string_value
        .find(decimal_separator)
        .map(|idx| string_value[..idx].chars().count())
        .unwrap_or(string_value.chars().count());
    if !pic.picture.contains(decimal_separator)
        || decimal_pos == string_value.chars().count().saturating_sub(1)
    {
        string_value = string_value.chars().take(string_value.chars().count() - 1).collect();
    }

    if let Some(exp) = exponent {
        let mut string_exponent = make_string(exp as f64, 0);
        let pad_exponent = pic
            .minimum_exponent_size
            .saturating_sub(string_exponent.chars().count());
        if pad_exponent > 0 {
            string_exponent = format!("{}{}", repeat_token(&props.zero_digit, pad_exponent), string_exponent);
        }
        let exp_sign = if exp < 0 {
            props.minus_sign.clone()
        } else {
            String::new()
        };
        string_value = format!(
            "{}{}{}{}",
            string_value, props.exponent_separator, exp_sign, string_exponent
        );
    }

    Ok(JsonValue::String(format!(
        "{}{}{}",
        pic.prefix, string_value, pic.suffix
    )))
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
        let array = JsonValue::Array(JsonArray::new(vec![JsonValue::Number(2.0)], false, false));
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

    #[test]
    fn format_base_default() {
        let result = format_base(&JsonValue::Number(100.0), &JsonValue::Undefined).unwrap();
        assert!(matches!(result, JsonValue::String(ref s) if s == "100"));
    }

    #[test]
    fn format_base_binary() {
        let result = format_base(&JsonValue::Number(100.0), &JsonValue::Number(2.0)).unwrap();
        assert!(matches!(result, JsonValue::String(ref s) if s == "1100100"));
    }

    #[test]
    fn format_number_basic() {
        let result = format_number(
            &JsonValue::Number(12345.6),
            &JsonValue::String("#,###.00".to_owned()),
            &JsonValue::Undefined,
        )
        .unwrap();
        assert!(matches!(result, JsonValue::String(ref s) if s == "12,345.60"));
    }
}
