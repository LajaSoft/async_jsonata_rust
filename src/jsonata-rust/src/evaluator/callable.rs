use std::collections::HashMap;
use std::sync::Arc;

use futures::executor::block_on;
use futures::future::BoxFuture;
use regex::{Regex, RegexBuilder};
use serde_json::Value;
use time::{Duration, OffsetDateTime, UtcOffset};

use crate::error::Error;
use crate::types::{
    FunctionContext, JsonArray, JsonCallable, JsonFunction, JsonObject, JsonValue, JsonataFocus,
};

use super::transform::eval_transform_apply;
use super::{eval, Bindings};

pub(super) fn eval_variable(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let raw = node
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_default();

    if raw == "$" {
        return Ok(focus.clone());
    }
    if raw.is_empty() {
        return Ok(focus.clone());
    }

    if raw == "$$" {
        return Ok(input.clone());
    }

    if let Some(value) = bindings.get(raw) {
        return Ok(value.clone());
    }

    let name = raw.trim_start_matches('$');
    if let Some(value) = bindings.get(name) {
        return Ok(value.clone());
    }

    if let Some(func) = functions.get(name) {
        return Ok(JsonValue::Function(func.clone()));
    }

    Ok(JsonValue::Undefined)
}

#[derive(Clone)]
struct RegexMatcherCallable {
    regex: Arc<Regex>,
    input: Option<String>,
    offset: usize,
}

impl RegexMatcherCallable {
    fn root(regex: Arc<Regex>) -> Self {
        Self {
            regex,
            input: None,
            offset: 0,
        }
    }
}

impl JsonCallable for RegexMatcherCallable {
    fn call(
        &self,
        _ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, crate::types::JsonError>> {
        let input = match &self.input {
            Some(value) => value.clone(),
            None => match args.first() {
                Some(JsonValue::String(text)) => text.clone(),
                _ => return Box::pin(async { Ok(JsonValue::Undefined) }),
            },
        };

        let requested_offset = if self.input.is_some() {
            self.offset
        } else {
            match args.get(1) {
                Some(JsonValue::Number(value)) if *value >= 0.0 => *value as usize,
                _ => 0usize,
            }
        };

        let regex = Arc::clone(&self.regex);
        Box::pin(async move { Ok(build_regex_match_value(regex, input, requested_offset)) })
    }

    fn arity(&self) -> Option<usize> {
        Some(2)
    }

    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}

fn build_regex_match_value(regex: Arc<Regex>, input: String, offset: usize) -> JsonValue {
    if offset > input.len() {
        return JsonValue::Undefined;
    }

    let Some(captures) = regex.captures_at(&input, offset) else {
        return JsonValue::Undefined;
    };
    let Some(whole) = captures.get(0) else {
        return JsonValue::Undefined;
    };

    let match_text = whole.as_str().to_owned();
    let match_start = whole.start();
    let match_end = whole.end();

    let mut groups = Vec::new();
    for index in 1..captures.len() {
        if let Some(group) = captures.get(index) {
            groups.push(JsonValue::String(group.as_str().to_owned()));
        }
    }

    let next_offset = match_end;
    let next_callable = JsonFunction::new(Arc::new(RegexMatcherCallable {
        regex,
        input: Some(input),
        offset: next_offset,
    }));

    JsonValue::Object(JsonObject(vec![
        ("match".to_owned(), JsonValue::String(match_text)),
        ("start".to_owned(), JsonValue::Number(match_start as f64)),
        ("end".to_owned(), JsonValue::Number((match_end - 1) as f64)),
        (
            "groups".to_owned(),
            JsonValue::Array(JsonArray::new(groups, false, false)),
        ),
        ("next".to_owned(), JsonValue::Function(next_callable)),
    ]))
}

pub(super) fn eval_regex(node: &Value) -> Result<JsonValue, Error> {
    let payload = node
        .get("value")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::new("E2031", "Regex node missing payload"))?;
    let pattern = payload
        .get("pattern")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::new("E2032", "Regex node missing pattern"))?;
    let flags = payload
        .get("flags")
        .and_then(Value::as_str)
        .unwrap_or_default();

    let mut builder = RegexBuilder::new(pattern);
    if flags.contains('i') {
        builder.case_insensitive(true);
    }
    if flags.contains('m') {
        builder.multi_line(true);
    }
    let regex = builder
        .build()
        .map_err(|err| Error::new("D1004", format!("Invalid regex: {err}")))?;

    Ok(JsonValue::Function(JsonFunction::new(Arc::new(
        RegexMatcherCallable::root(Arc::new(regex)),
    ))))
}

pub(super) fn eval_function(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let procedure = node
        .get("procedure")
        .ok_or_else(|| Error::new("E2006", "Function node missing procedure"))?;
    if let Some(raw) = procedure.get("value").and_then(Value::as_str) {
        let prefixed = format!("${raw}");
        let has_function_override = matches!(
            bindings.get(raw).or_else(|| bindings.get(prefixed.as_str())),
            Some(JsonValue::Function(_))
        );
        if !has_function_override && raw == "millis" {
            if let Some(JsonValue::Number(eval_millis)) = bindings.get("__jsonata_eval_millis") {
                return Ok(JsonValue::Number(*eval_millis));
            }
        }
        if !has_function_override && raw == "now" {
            if let Some(JsonValue::Number(eval_millis)) = bindings.get("__jsonata_eval_millis") {
                let arguments = node
                    .get("arguments")
                    .and_then(Value::as_array)
                    .ok_or_else(|| Error::new("E2007", "Function node missing arguments"))?;
                let picture = arguments
                    .first()
                    .map(|arg| eval(arg, input, focus, functions, bindings))
                    .transpose()?
                    .and_then(|value| match value {
                        JsonValue::String(text) => Some(text),
                        _ => None,
                    });
                let timezone = arguments
                    .get(1)
                    .map(|arg| eval(arg, input, focus, functions, bindings))
                    .transpose()?
                    .and_then(|value| match value {
                        JsonValue::String(text) => Some(text),
                        _ => None,
                    });
                return Ok(JsonValue::String(format_now_from_millis(
                    *eval_millis as i64,
                    picture.as_deref(),
                    timezone.as_deref(),
                )));
            }
        }
    }

    let callable = resolve_callable(procedure, input, focus, functions, bindings)?;
    let arguments = node
        .get("arguments")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2007", "Function node missing arguments"))?;

    let mut args = Vec::with_capacity(arguments.len());
    for arg in arguments {
        args.push(eval(arg, input, focus, functions, bindings)?);
    }
    if args.is_empty() {
        args.push(focus.clone());
    }

    let ctx = FunctionContext::with_focus(JsonataFocus::new(focus.clone()));
    match block_on(callable.call(ctx, args)) {
        Ok(value) => Ok(value),
        Err(err) => {
            if err.code == "J9001" {
                let mut details = err.message.clone();
                if let Some(position) = node.get("position").and_then(Value::as_i64) {
                    details.push_str(format!(";position:{position}").as_str());
                }
                if let Some(token) = procedure
                    .get("value")
                    .and_then(Value::as_str)
                    .or_else(|| procedure.get("token").and_then(Value::as_str))
                {
                    details.push_str(format!(";token:{token}").as_str());
                }
                return Err(Error::new(err.code, details));
            }
            Err(Error::from(err))
        }
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

fn format_now_iso_utc(now: OffsetDateTime) -> String {
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

fn format_now_custom(now: OffsetDateTime, timezone: Option<&str>) -> String {
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

fn format_now_from_millis(millis: i64, picture: Option<&str>, timezone: Option<&str>) -> String {
    let seconds = millis.div_euclid(1000);
    let millis_remainder = millis.rem_euclid(1000);
    let base = OffsetDateTime::from_unix_timestamp(seconds).unwrap_or_else(|_| OffsetDateTime::now_utc());
    let now = base + Duration::milliseconds(millis_remainder);
    if picture == Some("[h]:[M01][P] [z]") {
        return format_now_custom(now, timezone);
    }
    format_now_iso_utc(now)
}

pub(super) fn resolve_callable(
    procedure: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonFunction, Error> {
    let procedure_type = procedure
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();

    match procedure_type {
        "variable" => {
            let raw = procedure
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if let Some(JsonValue::Function(func)) = bindings.get(raw) {
                return Ok(func.clone());
            }

            let name = raw.trim_start_matches('$').to_owned();
            if let Some(JsonValue::Function(func)) = bindings.get(name.as_str()) {
                return Ok(func.clone());
            }

            functions
                .get(name.as_str())
                .cloned()
                .ok_or_else(|| Error::new("T1006", format!("Unknown function: {name}")))
        }
        "path" => {
            let resolved = eval(procedure, input, focus, functions, bindings)?;
            if let JsonValue::Function(func) = resolved {
                return Ok(func);
            }

            let steps = procedure
                .get("steps")
                .and_then(Value::as_array)
                .ok_or_else(|| Error::new("E2008", "Procedure path missing steps"))?;
            if steps.len() != 1 {
                return Err(Error::new("T1006", "Procedure is not callable"));
            }

            let step = &steps[0];
            let name = step
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim_start_matches('$')
                .to_owned();

            functions
                .get(name.as_str())
                .cloned()
                .ok_or_else(|| Error::new("T1006", "Procedure is not callable"))
        }
        _ => {
            let value = eval(procedure, input, focus, functions, bindings)?;
            match value {
                JsonValue::Function(func) => Ok(func),
                _ => Err(Error::new("T1006", "Procedure is not callable")),
            }
        }
    }
}

pub(super) fn eval_apply(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let lhs = node
        .get("lhs")
        .ok_or_else(|| Error::new("E2009", "Apply node missing lhs"))?;
    let rhs = node
        .get("rhs")
        .ok_or_else(|| Error::new("E2010", "Apply node missing rhs"))?;

    let base = eval(lhs, input, focus, functions, bindings)?;

    if rhs.get("type").and_then(Value::as_str) == Some("transform") {
        return eval_transform_apply(rhs, input, &base, functions, bindings);
    }

    if rhs.get("type").and_then(Value::as_str) == Some("function") {
        let procedure = rhs
            .get("procedure")
            .ok_or_else(|| Error::new("E2011", "Apply function missing procedure"))?;
        let callable = resolve_callable(procedure, input, &base, functions, bindings)?;

        let mut args = vec![base.clone()];
        if let Some(extra_args) = rhs.get("arguments").and_then(Value::as_array) {
            for arg in extra_args {
                args.push(eval(arg, input, focus, functions, bindings)?);
            }
        }

        let ctx = FunctionContext::with_focus(JsonataFocus::new(base));
        return block_on(callable.call(ctx, args)).map_err(Error::from);
    }

    let candidate = eval(rhs, input, &base, functions, bindings)?;
    match candidate {
        JsonValue::Function(callable) => {
            let ctx = FunctionContext::with_focus(JsonataFocus::new(base.clone()));
            block_on(callable.call(ctx, vec![base])).map_err(Error::from)
        }
        _ => Err(Error::new("T1006", "Right side of apply is not callable")),
    }
}
