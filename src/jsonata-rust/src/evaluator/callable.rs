use std::collections::HashMap;
use std::sync::Arc;

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

const CUSTOM_REGEX_FACTORY_BINDING: &str = "__jsonata_regex_engine_factory";

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
        if node.get("id").and_then(Value::as_str) == Some("$$") {
            return Ok(unwrap_outer_wrapper(input));
        }
        return Ok(unwrap_outer_wrapper(focus));
    }
    if raw.is_empty() {
        return Ok(unwrap_outer_wrapper(focus));
    }

    if raw == "$$" {
        return Ok(unwrap_outer_wrapper(input));
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

/// Mirrors upstream `evaluateVariable`: the root `$`/`$$` reference returns the
/// original document. When the document is a plain JSON array the engine wraps
/// it in a singleton outer-wrapper sequence; unwrap that here so `$` yields the
/// original array rather than the wrapper.
fn unwrap_outer_wrapper(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Array(array) if array.outer_wrapper && array.elements.len() == 1 => {
            array.elements[0].clone()
        }
        other => other.clone(),
    }
}

#[derive(Clone)]
struct RegexMatcherCallable {
    regex: Arc<Regex>,
    input: Option<String>,
    offset: usize,
}

#[derive(Clone)]
enum PartialArg {
    Placeholder,
    Value(JsonValue),
}

#[derive(Clone)]
struct PartialCallable {
    target: JsonFunction,
    template: Vec<PartialArg>,
    captured_focus: JsonValue,
}

impl JsonCallable for PartialCallable {
    fn call(
        &self,
        ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, crate::types::JsonError>> {
        let mut consumed = 0usize;
        let mut merged = Vec::with_capacity(self.template.len() + args.len());
        for slot in &self.template {
            match slot {
                PartialArg::Placeholder => {
                    merged.push(args.get(consumed).cloned().unwrap_or(JsonValue::Undefined));
                    consumed += 1;
                }
                PartialArg::Value(value) => merged.push(value.clone()),
            }
        }
        while consumed < args.len() {
            merged.push(args[consumed].clone());
            consumed += 1;
        }

        let focus = ctx
            .focus()
            .map(|focus| focus.input.clone())
            .unwrap_or_else(|| self.captured_focus.clone());
        let call_ctx = FunctionContext::with_focus(JsonataFocus::new(focus));
        self.target.call(call_ctx, merged)
    }

    fn arity(&self) -> Option<usize> {
        Some(
            self.template
                .iter()
                .filter(|arg| matches!(arg, PartialArg::Placeholder))
                .count(),
        )
    }

    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
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
        ("end".to_owned(), JsonValue::Number(match_end as f64)),
        (
            "groups".to_owned(),
            JsonValue::Array(JsonArray::new(groups, false, false)),
        ),
        ("next".to_owned(), JsonValue::Function(next_callable)),
    ]))
}

pub(super) fn eval_regex<'a>(
    node: &'a Value,
    focus: &'a JsonValue,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    Box::pin(async move {
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

    if let Some(JsonValue::Function(factory)) = bindings.get(CUSTOM_REGEX_FACTORY_BINDING) {
        let ctx = FunctionContext::with_focus(JsonataFocus::new(focus.clone()));
        let args = vec![
            JsonValue::String(pattern.to_owned()),
            JsonValue::String(flags.to_owned()),
        ];
        let produced = factory.call(ctx, args).await.map_err(Error::from)?;
        return match produced {
            JsonValue::Function(matcher) => Ok(JsonValue::Function(matcher)),
            other => Err(Error::new(
                "D1004",
                format!("Custom RegexEngine factory must return function, got {:?}", other),
            )),
        };
    }

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
    })
}

pub(super) fn eval_function<'a>(
    node: &'a Value,
    input: &'a JsonValue,
    focus: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    eval_function_internal(node, input, focus, functions, bindings, None, true)
}

pub(super) fn eval_tail_call<'a>(
    node: &'a Value,
    input: &'a JsonValue,
    focus: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    eval_function_internal(node, input, focus, functions, bindings, None, false)
}

pub(super) fn eval_function_with_applyto<'a>(
    node: &'a Value,
    input: &'a JsonValue,
    focus: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
    applyto: &'a JsonValue,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    eval_function_internal(node, input, focus, functions, bindings, Some(applyto), true)
}

fn eval_function_internal<'a>(
    node: &'a Value,
    input: &'a JsonValue,
    focus: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
    applyto: Option<&'a JsonValue>,
    drive_thunks: bool,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    Box::pin(async move {
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
                let picture = match arguments.first() {
                    Some(arg) => match eval(arg, input, focus, functions, bindings).await? {
                        JsonValue::String(text) => Some(text),
                        _ => None,
                    },
                    None => None,
                };
                let timezone = match arguments.get(1) {
                    Some(arg) => match eval(arg, input, focus, functions, bindings).await? {
                        JsonValue::String(text) => Some(text),
                        _ => None,
                    },
                    None => None,
                };
                return Ok(JsonValue::String(format_now_from_millis(
                    *eval_millis as i64,
                    picture.as_deref(),
                    timezone.as_deref(),
                )));
            }
        }
        if !has_function_override && raw == "eval" {
            return eval_eval(node, input, focus, functions, bindings, applyto).await;
        }
    }

    let callable = resolve_callable(procedure, input, focus, functions, bindings).await?;
    let arguments = node
        .get("arguments")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2007", "Function node missing arguments"))?;

    let mut args = Vec::with_capacity(arguments.len());
    for arg in arguments {
        args.push(eval(arg, input, focus, functions, bindings).await?);
    }

    // A builtin may declare a signature with a context (`-`) modifier; in that
    // case argument validation injects the focus value for the missing first
    // argument (e.g. `$uppercase()` or `$substringBefore(" ")` as a path step).
    let builtin_signature = procedure
        .get("value")
        .and_then(Value::as_str)
        .filter(|raw| {
            !matches!(
                bindings.get(*raw).or_else(|| bindings.get(format!("${raw}").as_str())),
                Some(JsonValue::Function(_))
            )
        })
        .and_then(builtin_signature);

    if let Some(signature) = builtin_signature {
        if let Some(context_value) = applyto {
            args.insert(0, context_value.clone());
        }
        let validated = signature.validate(args, focus).map_err(|err| {
            let mut details = err.message.clone();
            if let Some(position) = node.get("position").and_then(Value::as_i64) {
                details.push_str(format!(";position:{position}").as_str());
            }
            if let Some(token) = procedure.get("value").and_then(Value::as_str) {
                details.push_str(format!(";token:{token}").as_str());
            }
            Error::new(err.code, details)
        })?;
        let ctx = FunctionContext::with_focus(JsonataFocus::new(focus.clone()));
        return finish_call(callable, ctx, validated, node, procedure, drive_thunks).await;
    }

    let arity = callable.arity();
    if let Some(context_value) = applyto {
        // For `lhs ~> $func(...)` the lhs is always prepended as the first
        // argument (matching the JSONata reference engine).
        args.insert(0, context_value.clone());
    } else if args.is_empty() {
        if arity.is_none() || arity.unwrap_or(0) > 0 {
            args.push(focus.clone());
        }
    }

    let ctx = FunctionContext::with_focus(JsonataFocus::new(focus.clone()));
    finish_call(callable, ctx, args, node, procedure, drive_thunks).await
    })
}

/// Invokes a callable and decorates any error with the function's
/// position/token, matching the reference engine.
///
/// When `drive_thunks` is true (an ordinary call site) the result is driven to a
/// final value through the single tail-call trampoline
/// ([`JsonFunction::call_forced`]). When false (an `eval_tail_call` step) the raw
/// result — possibly itself a tail-call thunk — is returned so the *outer*
/// trampoline can drive it, which is what keeps tail recursion iterative.
fn finish_call<'a>(
    callable: JsonFunction,
    ctx: FunctionContext,
    args: Vec<JsonValue>,
    node: &'a Value,
    procedure: &'a Value,
    drive_thunks: bool,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    Box::pin(async move {
    let outcome = if drive_thunks {
        callable.call_forced(ctx, args).await
    } else {
        callable.call(ctx, args).await
    };
    match outcome {
        Ok(value) => Ok(value),
        Err(err) => {
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
            Err(Error::new(err.code, details))
        }
    }
    })
}

/// Returns the parsed signature for a built-in function, if one is declared
/// with a context (`-`) modifier or type constraints that affect evaluation.
///
/// The compiled signatures (each of which owns a compiled `Regex`) are memoised
/// in a process-wide table built on first use. Re-parsing them per call used to
/// dominate the cost of mapping a built-in over a large sequence
/// (e.g. `[1..2e5].$string()`), because `Signature::parse` compiles a regex.
fn builtin_signature(name: &str) -> Option<&'static super::signature::Signature> {
    use std::collections::HashMap;
    use std::sync::OnceLock;

    static SIGNATURES: OnceLock<HashMap<&'static str, super::signature::Signature>> =
        OnceLock::new();

    // Source signature strings; each is parsed exactly once into the table.
    const SPECS: &[(&str, &str)] = &[
        ("string", "<x-b?:s>"),
        ("substring", "<s-nn?:s>"),
        ("substringBefore", "<s-s:s>"),
        ("substringAfter", "<s-s:s>"),
        ("lowercase", "<s-:s>"),
        ("uppercase", "<s-:s>"),
        ("length", "<s-:n>"),
        ("trim", "<s-:s>"),
        ("pad", "<s-ns?:s>"),
        ("contains", "<s-(sf):b>"),
        ("split", "<s-(sf)n?:a<s>>"),
        ("formatNumber", "<n-so?:s>"),
        ("formatBase", "<n-n?:s>"),
        ("number", "<(nsb)-:n>"),
        ("floor", "<n-:n>"),
        ("ceil", "<n-:n>"),
        ("round", "<n-n?:n>"),
        ("abs", "<n-:n>"),
        ("sqrt", "<n-:n>"),
        ("power", "<n-n:n>"),
        ("boolean", "<x-:b>"),
        ("not", "<x-:b>"),
        ("sift", "<o-f?:o>"),
        ("keys", "<x-:a<s>>"),
        ("lookup", "<x-s:x>"),
        ("spread", "<x-:a<o>>"),
        ("each", "<o-f:a>"),
        ("base64encode", "<s-:s>"),
        ("base64decode", "<s-:s>"),
        ("encodeUrlComponent", "<s-:s>"),
        ("encodeUrl", "<s-:s>"),
        ("decodeUrlComponent", "<s-:s>"),
        ("decodeUrl", "<s-:s>"),
        ("exists", "<x:b>"),
        ("type", "<x:s>"),
        ("map", "<af>"),
        ("filter", "<af>"),
        ("single", "<af?>"),
        ("sort", "<af?:a>"),
    ];

    let table = SIGNATURES.get_or_init(|| {
        SPECS
            .iter()
            .filter_map(|(name, sig)| {
                super::signature::Signature::parse(sig).map(|parsed| (*name, parsed))
            })
            .collect()
    });
    table.get(name)
}

fn eval_eval<'a>(
    node: &'a Value,
    input: &'a JsonValue,
    focus: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
    applyto: Option<&'a JsonValue>,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    Box::pin(async move {
    let arguments = node
        .get("arguments")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2007", "Function node missing arguments"))?;

    // Evaluate the explicit arguments.
    let mut args: Vec<JsonValue> = Vec::with_capacity(arguments.len());
    for arg in arguments {
        args.push(eval(arg, input, focus, functions, bindings).await?);
    }

    // Handle the chained application form `X ~> $eval(...)` where the lhs is
    // prepended as the first argument.
    if let Some(context_value) = applyto {
        if args.is_empty() {
            args.push(context_value.clone());
        } else {
            args.insert(0, context_value.clone());
        }
    }

    let expr_arg = args.first().cloned().unwrap_or(JsonValue::Undefined);
    let expr_str = match expr_arg {
        JsonValue::String(text) => text,
        JsonValue::Undefined => return Ok(JsonValue::Undefined),
        _ => return Ok(JsonValue::Undefined),
    };

    // Determine the focus/input for the nested evaluation.
    let nested_input = match args.get(1) {
        Some(JsonValue::Undefined) | None => focus.clone(),
        Some(other) => {
            // If a JSON array is passed as focus, wrap it in a singleton
            // sequence so it is treated as a single input value.
            match other {
                JsonValue::Array(array) if !array.is_sequence => {
                    JsonValue::Array(JsonArray::new(vec![other.clone()], true, true))
                }
                _ => other.clone(),
            }
        }
    };

    let ast = match crate::parser::parse_expression(&expr_str, false) {
        Ok(ast) => ast,
        Err(err) => {
            return Err(Error::new(
                "D3120",
                format!("Syntax error in expression passed to function eval: {}", err.code),
            ));
        }
    };

    match eval(&ast, &nested_input, &nested_input, functions, bindings).await {
        Ok(value) => Ok(value),
        Err(err) => Err(Error::new("D3121", err.message().to_owned())),
    }
    })
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

pub(super) fn resolve_callable<'a>(
    procedure: &'a Value,
    input: &'a JsonValue,
    focus: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<JsonFunction, Error>> {
    Box::pin(async move {
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
            let resolved = eval(procedure, input, focus, functions, bindings).await?;
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
            let raw = step.get("value").and_then(Value::as_str).unwrap_or_default();
            let name = raw.trim_start_matches('$').to_owned();

            // A `$`-prefixed path step refers to a function/variable.
            if raw.starts_with('$') {
                if let Some(JsonValue::Function(func)) = bindings.get(name.as_str()) {
                    return Ok(func.clone());
                }
                return functions
                    .get(name.as_str())
                    .cloned()
                    .ok_or_else(|| Error::new("T1006", "Procedure is not callable"));
            }

            // A bare name (no `$`) is a path lookup against the data, not a
            // function reference. If it happens to name a built-in, the user
            // most likely forgot the leading `$` (T1005).
            if functions.contains_key(name.as_str()) || bindings.contains_key(name.as_str()) {
                return Err(Error::new(
                    "T1005",
                    format!("Attempted to invoke a non-function. Did you mean ${name}?"),
                ));
            }
            Err(Error::new("T1006", "Procedure is not callable"))
        }
        _ => {
            let value = eval(procedure, input, focus, functions, bindings).await?;
            match value {
                JsonValue::Function(func) => Ok(func),
                _ => Err(Error::new("T1006", "Procedure is not callable")),
            }
        }
    }
    })
}

pub(super) fn eval_apply<'a>(
    node: &'a Value,
    input: &'a JsonValue,
    focus: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    Box::pin(async move {
    let lhs = node
        .get("lhs")
        .ok_or_else(|| Error::new("E2009", "Apply node missing lhs"))?;
    let rhs = node
        .get("rhs")
        .ok_or_else(|| Error::new("E2010", "Apply node missing rhs"))?;

    let base = eval(lhs, input, focus, functions, bindings).await?;

    if rhs.get("type").and_then(Value::as_str) == Some("transform") {
        return eval_transform_apply(rhs, input, &base, functions, bindings).await;
    }

    if rhs.get("type").and_then(Value::as_str) == Some("function") {
        return eval_function_with_applyto(rhs, input, focus, functions, bindings, &base).await;
    }

    let candidate = eval(rhs, input, &base, functions, bindings).await?;
    match candidate {
        JsonValue::Function(callable) => {
            if let JsonValue::Function(first) = &base {
                // Function chaining: `func1 ~> func2` builds the composition
                // λ($x){ func2(func1($x)) }.
                return Ok(JsonValue::Function(JsonFunction::new(Arc::new(
                    ChainCallable {
                        first: first.clone(),
                        second: callable,
                    },
                ))));
            }
            let ctx = FunctionContext::with_focus(JsonataFocus::new(base.clone()));
            callable.call_forced(ctx, vec![base]).await.map_err(Error::from)
        }
        _ => Err(Error::new(
            "T2006",
            "The right side of the function application operator ~> must be a function",
        )),
    }
    })
}

#[derive(Clone)]
struct ChainCallable {
    first: JsonFunction,
    second: JsonFunction,
}

impl JsonCallable for ChainCallable {
    fn call(
        &self,
        ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, crate::types::JsonError>> {
        let first = self.first.clone();
        let second = self.second.clone();
        let focus = ctx
            .focus()
            .map(|focus| focus.input.clone())
            .unwrap_or(JsonValue::Undefined);
        Box::pin(async move {
            let first_ctx = FunctionContext::with_focus(JsonataFocus::new(focus.clone()));
            // Drive the first function's result to a value before feeding it to
            // the second; `second`'s own result is driven by whoever invoked the
            // chain (always through `call_forced`).
            let intermediate = first.call_forced(first_ctx, args).await?;
            let second_ctx = FunctionContext::with_focus(JsonataFocus::new(focus));
            second.call(second_ctx, vec![intermediate]).await
        })
    }

    fn arity(&self) -> Option<usize> {
        self.first.arity()
    }

    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}

pub(super) fn eval_partial<'a>(
    node: &'a Value,
    input: &'a JsonValue,
    focus: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    Box::pin(async move {
    let procedure = node
        .get("procedure")
        .ok_or_else(|| Error::new("E2033", "Partial node missing procedure"))?;
    let target = resolve_callable(procedure, input, focus, functions, bindings).await.map_err(|_| {
        // Distinguish "forgot the leading $" (the bare name matches a builtin
        // or variable) from a genuinely unknown procedure.
        let bare_name = if procedure.get("type").and_then(Value::as_str) == Some("path") {
            procedure
                .get("steps")
                .and_then(Value::as_array)
                .and_then(|steps| steps.first())
                .and_then(|step| step.get("value"))
                .and_then(Value::as_str)
        } else {
            procedure.get("value").and_then(Value::as_str)
        };
        if let Some(name) = bare_name {
            let stripped = name.trim_start_matches('$');
            if !name.starts_with('$')
                && (functions.contains_key(stripped) || bindings.contains_key(stripped))
            {
                return Error::new(
                    "T1007",
                    format!("Attempted to partially apply a non-function. Did you mean ${stripped}?"),
                );
            }
        }
        Error::new("T1008", "Attempted to partially apply a non-function")
    })?;
    let arguments = node
        .get("arguments")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2034", "Partial node missing arguments"))?;

    let mut template = Vec::with_capacity(arguments.len());
    for arg in arguments {
        let is_placeholder = arg.get("type").and_then(Value::as_str) == Some("operator")
            && arg.get("value").and_then(Value::as_str) == Some("?");
        if is_placeholder {
            template.push(PartialArg::Placeholder);
            continue;
        }
        template.push(PartialArg::Value(eval(arg, input, focus, functions, bindings).await?));
    }

    Ok(JsonValue::Function(JsonFunction::new(Arc::new(
        PartialCallable {
            target,
            template,
            captured_focus: focus.clone(),
        },
    ))))
    })
}
