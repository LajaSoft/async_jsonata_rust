use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;

use futures::executor::block_on;
use futures::future::BoxFuture;
use serde_json::Value;

use crate::error::Error;
use crate::functions::core;
use crate::types::{
    FunctionContext, JsonArray, JsonCallable, JsonError, JsonFunction, JsonObject, JsonValue,
    JsonataFocus,
};

type Bindings = HashMap<String, JsonValue>;

#[derive(Clone)]
struct LambdaCallable {
    arg_names: Vec<String>,
    body: Value,
    body_is_thunk: bool,
    captured_input: JsonValue,
    captured_focus: JsonValue,
    captured_bindings: Bindings,
    functions: HashMap<String, JsonFunction>,
}

impl JsonCallable for LambdaCallable {
    fn call(
        &self,
        ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
        let mut call_bindings = self.captured_bindings.clone();
        for (index, name) in self.arg_names.iter().enumerate() {
            let value = args.get(index).cloned().unwrap_or(JsonValue::Undefined);
            call_bindings.insert(name.clone(), value.clone());
            call_bindings.insert(format!("${name}"), value);
        }

        let focus = match ctx.focus() {
            Some(focus) => focus.input.clone(),
            None => self.captured_focus.clone(),
        };
        let body = self.body.clone();
        let body_is_thunk = self.body_is_thunk;
        let captured_input = self.captured_input.clone();
        let functions = self.functions.clone();

        Box::pin(async move {
            let handle = std::thread::spawn(move || {
                let mut value = eval(
                    &body,
                    &captured_input,
                    &focus,
                    &functions,
                    &call_bindings,
                )?;

                if body_is_thunk {
                    let mut depth = 0usize;
                    while let JsonValue::Function(callable) = value {
                        if depth >= 32 {
                            return Err(Error::new("U1001", "Tail-call trampoline depth exceeded"));
                        }
                        let ctx = FunctionContext::with_focus(JsonataFocus::new(focus.clone()));
                        value = block_on(callable.call(ctx, Vec::new())).map_err(Error::from)?;
                        depth += 1;
                    }
                }

                Ok(value)
            });

            let result = handle.join().map_err(|_| {
                JsonError::new("D3120", "Lambda execution failed: worker thread panicked")
            })?;

            result.map_err(|err| {
                JsonError::new(
                    "D3120",
                    format!("Lambda execution failed: {}: {}", err.code(), err.message()),
                )
            })
        })
    }

    fn arity(&self) -> Option<usize> {
        Some(self.arg_names.len())
    }

    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}

pub(crate) fn evaluate_expression(
    ast: &Value,
    input: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
) -> Result<JsonValue, Error> {
    let bindings = Bindings::new();
    eval(ast, input, input, functions, &bindings)
}

fn eval(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let node_type = node.get("type").and_then(Value::as_str).unwrap_or_default();

    match node_type {
        "path" => eval_path(node, input, focus, functions, bindings),
        "name" => {
            let name = node
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Ok(core::lookup(focus, name))
        }
        "variable" => eval_variable(node, input, focus, functions, bindings),
        "string" => Ok(JsonValue::String(
            node.get("value")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        )),
        "number" => Ok(JsonValue::Number(
            node.get("value").and_then(Value::as_f64).unwrap_or(0.0),
        )),
        "value" => Ok(json_value_from_serde(node.get("value").unwrap_or(&Value::Null))),
        "function" => eval_function(node, input, focus, functions, bindings),
        "binary" => eval_binary(node, input, focus, functions, bindings),
        "apply" => eval_apply(node, input, focus, functions, bindings),
        "block" => eval_block(node, input, focus, functions, bindings),
        "unary" => eval_unary(node, input, focus, functions, bindings),
        "bind" => {
            let (_, value) = eval_bind(node, input, focus, functions, bindings)?;
            Ok(value)
        }
        "lambda" => eval_lambda(node, input, focus, functions, bindings),
        "condition" => eval_condition(node, input, focus, functions, bindings),
        _ => Err(Error::new(
            "E2001",
            format!("Unsupported AST node type: {node_type}"),
        )),
    }
}

fn eval_path(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let mut current = focus.clone();
    let steps = node
        .get("steps")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2002", "Path node missing steps"))?;

    for step in steps {
        current = eval_path_step(step, input, &current, functions, bindings)?;
    }

    Ok(current)
}

fn eval_path_step(
    step: &Value,
    input: &JsonValue,
    current: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let step_type = step.get("type").and_then(Value::as_str).unwrap_or_default();

    let mut out = match step_type {
        "name" => {
            let key = step
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default();
            core::lookup(current, key)
        }
        "function" => eval_function(step, input, current, functions, bindings)?,
        "variable" => eval_variable(step, input, current, functions, bindings)?,
        "block" => eval_path_expr_step(step, input, current, functions, bindings)?,
        "condition" => eval_path_expr_step(step, input, current, functions, bindings)?,
        "number" => JsonValue::Number(step.get("value").and_then(Value::as_f64).unwrap_or(0.0)),
        "string" => JsonValue::String(
            step.get("value")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        ),
        "sort" => apply_sort_step(step, input, current, functions, bindings)?,
        "wildcard" => apply_wildcard(current),
        other => {
            return Err(Error::new(
                "E2003",
                format!("Unsupported path step type: {other}"),
            ))
        }
    };

    if let Some(index) = step.get("index") {
        out = apply_index(&out, index);
    }

    if let Some(stages) = step.get("stages").and_then(Value::as_array) {
        for stage in stages {
            out = apply_stage(stage, input, &out, functions, bindings)?;
        }
    }

    Ok(out)
}

fn apply_stage(
    stage: &Value,
    input: &JsonValue,
    current: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let stage_type = stage
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();

    match stage_type {
        "filter" => {
            let expr = stage
                .get("expr")
                .ok_or_else(|| Error::new("E2004", "Filter stage missing expr"))?;

            if let Some(index) = extract_filter_index(expr) {
                return Ok(apply_index(current, &Value::Number(index.into())));
            }

            let mut kept = Vec::new();
            for item in to_sequence(current) {
                let predicate = eval(expr, input, &item, functions, bindings)?;
                if is_truthy(&predicate) {
                    kept.push(item);
                }
            }
            Ok(from_sequence(kept))
        }
        "index" => {
            let index = stage.get("value").unwrap_or(&Value::Null);
            Ok(apply_index(current, index))
        }
        other => Err(Error::new(
            "E2005",
            format!("Unsupported stage type: {other}"),
        )),
    }
}

fn eval_path_expr_step(
    step: &Value,
    input: &JsonValue,
    current: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let values = to_sequence(current);
    if values.is_empty() {
        return Ok(JsonValue::Undefined);
    }

    let mut out = Vec::new();
    for item in values {
        let value = eval(step, input, &item, functions, bindings)?;
        if value.is_undefined() {
            continue;
        }
        match value {
            JsonValue::Array(array) if array.is_sequence => {
                for element in array.elements {
                    out.push(element);
                }
            }
            other => out.push(other),
        }
    }

    Ok(from_sequence(out))
}

fn extract_filter_index(expr: &Value) -> Option<i64> {
    if expr.get("type").and_then(Value::as_str) == Some("number") {
        return numeric_index(expr.get("value"));
    }

    if expr.get("type").and_then(Value::as_str) == Some("value") {
        return numeric_index(expr.get("value"));
    }

    None
}

fn numeric_index(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    if let Some(index) = value.as_i64() {
        return Some(index);
    }
    let float = value.as_f64()?;
    if float.fract() != 0.0 {
        return None;
    }
    Some(float as i64)
}

fn apply_index(current: &JsonValue, index: &Value) -> JsonValue {
    let Some(idx) = index.as_i64() else {
        return JsonValue::Undefined;
    };

    let seq = to_sequence(current);
    if seq.is_empty() {
        return JsonValue::Undefined;
    }

    let position = if idx < 0 {
        let from_end = seq.len() as i64 + idx;
        if from_end < 0 {
            return JsonValue::Undefined;
        }
        from_end as usize
    } else {
        idx as usize
    };

    seq.get(position).cloned().unwrap_or(JsonValue::Undefined)
}

fn eval_variable(
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

fn eval_function(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let procedure = node
        .get("procedure")
        .ok_or_else(|| Error::new("E2006", "Function node missing procedure"))?;

    let callable = resolve_callable(procedure, input, focus, functions, bindings)?;
    let arguments = node
        .get("arguments")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2007", "Function node missing arguments"))?;

    let mut args = Vec::with_capacity(arguments.len());
    for arg in arguments {
        args.push(eval(arg, input, focus, functions, bindings)?);
    }

    let ctx = FunctionContext::with_focus(JsonataFocus::new(focus.clone()));
    block_on(callable.call(ctx, args)).map_err(Error::from)
}

fn resolve_callable(
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
            let name = procedure
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim_start_matches('$')
                .to_owned();
            functions
                .get(name.as_str())
                .cloned()
                .ok_or_else(|| Error::new("T1006", format!("Unknown function: {name}")))
        }
        "path" => {
            let steps = procedure
                .get("steps")
                .and_then(Value::as_array)
                .ok_or_else(|| Error::new("E2008", "Procedure path missing steps"))?;
            if steps.len() != 1 {
                return Err(Error::new("T1006", "Function procedure path must be a single name"));
            }

            let step = &steps[0];
            let step_type = step.get("type").and_then(Value::as_str).unwrap_or_default();
            let name = match step_type {
                "name" | "variable" => step
                    .get("value")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim_start_matches('$')
                    .to_owned(),
                _ => {
                    return Err(Error::new(
                        "T1006",
                        format!("Unsupported function procedure step: {step_type}"),
                    ))
                }
            };

            functions
                .get(name.as_str())
                .cloned()
                .ok_or_else(|| Error::new("T1006", format!("Unknown function: {name}")))
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

fn eval_apply(
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

fn eval_binary(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let op = node
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let lhs = node
        .get("lhs")
        .ok_or_else(|| Error::new("E2012", "Binary node missing lhs"))?;
    let rhs = node
        .get("rhs")
        .ok_or_else(|| Error::new("E2013", "Binary node missing rhs"))?;

    let left = eval(lhs, input, focus, functions, bindings)?;
    let right = eval(rhs, input, focus, functions, bindings)?;

    match op {
        "+" => number_binop(&left, &right, |a, b| a + b),
        "-" => number_binop(&left, &right, |a, b| a - b),
        "*" => number_binop(&left, &right, |a, b| a * b),
        "/" => number_binop(&left, &right, |a, b| a / b),
        "=" => Ok(JsonValue::Bool(values_equal(&left, &right))),
        "!=" => Ok(JsonValue::Bool(!values_equal(&left, &right))),
        ">" => number_cmp(&left, &right, |a, b| a > b),
        ">=" => number_cmp(&left, &right, |a, b| a >= b),
        "<" => number_cmp(&left, &right, |a, b| a < b),
        "<=" => number_cmp(&left, &right, |a, b| a <= b),
        "and" => Ok(JsonValue::Bool(is_truthy(&left) && is_truthy(&right))),
        "or" => Ok(JsonValue::Bool(is_truthy(&left) || is_truthy(&right))),
        _ => Err(Error::new(
            "E2014",
            format!("Unsupported binary operator: {op}"),
        )),
    }
}

fn eval_block(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let expressions = node
        .get("expressions")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2015", "Block node missing expressions"))?;

    let mut local_bindings = bindings.clone();
    let mut last = JsonValue::Undefined;
    for expr in expressions {
        if expr.get("type").and_then(Value::as_str) == Some("bind") {
            let (name, value) = eval_bind(expr, input, focus, functions, &local_bindings)?;
            local_bindings.insert(name.clone(), value.clone());
            local_bindings.insert(format!("${name}"), value.clone());
            last = value;
            continue;
        }
        last = eval(expr, input, focus, functions, &local_bindings)?;
    }

    Ok(last)
}

fn eval_unary(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let op = node
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_default();

    if op == "[" {
        let expressions = node
            .get("expressions")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::new("E2016", "Array unary missing expressions"))?;
        let mut out = Vec::with_capacity(expressions.len());
        for expr in expressions {
            out.push(eval(expr, input, focus, functions, bindings)?);
        }
        return Ok(JsonValue::Array(JsonArray::new(out, false, false)));
    }

    if op == "{" {
        let pairs = node
            .get("lhs")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::new("E2019", "Object unary missing lhs"))?;
        let mut object = JsonObject(Vec::new());
        for pair in pairs {
            let pair_values = pair
                .as_array()
                .ok_or_else(|| Error::new("E2020", "Object pair must be array"))?;
            if pair_values.len() != 2 {
                return Err(Error::new("E2021", "Object pair must contain key and value"));
            }

            let key_value = eval(&pair_values[0], input, focus, functions, bindings)?;
            let keys = object_keys_from_value(&key_value);
            if keys.is_empty() {
                continue;
            }
            let value = eval(&pair_values[1], input, focus, functions, bindings)?;
            let value = materialize_value(&value);
            for key in keys {
                upsert_object_property(&mut object, key, value.clone());
            }
        }
        return Ok(JsonValue::Object(object));
    }

    if op == "-" {
        let expr = node
            .get("expression")
            .ok_or_else(|| Error::new("E2017", "Unary minus missing expression"))?;
        let value = eval(expr, input, focus, functions, bindings)?;
        if let Some(num) = to_number(&value) {
            return Ok(JsonValue::Number(-num));
        }
        return Ok(JsonValue::Undefined);
    }

    Err(Error::new(
        "E2018",
        format!("Unsupported unary operator: {op}"),
    ))
}

fn number_binop(left: &JsonValue, right: &JsonValue, op: fn(f64, f64) -> f64) -> Result<JsonValue, Error> {
    let Some(a) = to_number(left) else {
        return Ok(JsonValue::Undefined);
    };
    let Some(b) = to_number(right) else {
        return Ok(JsonValue::Undefined);
    };
    Ok(JsonValue::Number(op(a, b)))
}

fn eval_bind(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<(String, JsonValue), Error> {
    let lhs = node
        .get("lhs")
        .ok_or_else(|| Error::new("E2022", "Bind node missing lhs"))?;
    let rhs = node
        .get("rhs")
        .ok_or_else(|| Error::new("E2023", "Bind node missing rhs"))?;

    let name = lhs
        .get("value")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::new("E2024", "Bind lhs must be variable"))?
        .trim_start_matches('$')
        .to_owned();

    if name.is_empty() {
        return Err(Error::new("E2025", "Bind variable name is empty"));
    }

    let value = eval(rhs, input, focus, functions, bindings)?;
    Ok((name, value))
}

fn eval_lambda(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let arguments = node
        .get("arguments")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut arg_names = Vec::with_capacity(arguments.len());
    for arg in arguments {
        if let Some(name) = extract_lambda_arg_name(&arg) {
            arg_names.push(name);
        }
    }

    let body = node
        .get("body")
        .cloned()
        .ok_or_else(|| Error::new("E2026", "Lambda missing body"))?;
    let body_is_thunk = body.get("thunk").and_then(Value::as_bool).unwrap_or(false);

    let callable = LambdaCallable {
        arg_names,
        body,
        body_is_thunk,
        captured_input: input.clone(),
        captured_focus: focus.clone(),
        captured_bindings: bindings.clone(),
        functions: functions.clone(),
    };

    Ok(JsonValue::Function(JsonFunction::new(Arc::new(callable))))
}

fn eval_condition(
    node: &Value,
    input: &JsonValue,
    focus: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let condition = node
        .get("condition")
        .ok_or_else(|| Error::new("E2027", "Condition node missing condition"))?;
    let then_branch = node
        .get("then")
        .ok_or_else(|| Error::new("E2028", "Condition node missing then"))?;

    let predicate = eval(condition, input, focus, functions, bindings)?;
    if is_truthy(&predicate) {
        return eval(then_branch, input, focus, functions, bindings);
    }

    if let Some(else_branch) = node.get("else") {
        return eval(else_branch, input, focus, functions, bindings);
    }

    Ok(JsonValue::Undefined)
}

fn apply_sort_step(
    step: &Value,
    input: &JsonValue,
    current: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<JsonValue, Error> {
    let terms = step
        .get("terms")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2029", "Sort step missing terms"))?;

    let mut values = to_sequence(current);
    values.sort_by(|left, right| {
        for term in terms {
            let descending = term
                .get("descending")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let expr = match term.get("expression") {
                Some(expr) => expr,
                None => continue,
            };

            let left_value = eval(expr, input, left, functions, bindings).ok();
            let right_value = eval(expr, input, right, functions, bindings).ok();
            let mut ordering = compare_sort_values(left_value.as_ref(), right_value.as_ref());
            if descending {
                ordering = ordering.reverse();
            }
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
        Ordering::Equal
    });

    Ok(from_sequence(values))
}

fn apply_wildcard(current: &JsonValue) -> JsonValue {
    match current {
        JsonValue::Object(JsonObject(entries)) => {
            let values = entries.iter().map(|(_, value)| value.clone()).collect();
            from_sequence(values)
        }
        JsonValue::Array(array) => {
            let mut values = Vec::new();
            for item in &array.elements {
                match item {
                    JsonValue::Object(JsonObject(entries)) => {
                        for (_, value) in entries {
                            values.push(value.clone());
                        }
                    }
                    other => values.push(other.clone()),
                }
            }
            from_sequence(values)
        }
        _ => JsonValue::Undefined,
    }
}

fn compare_sort_values(left: Option<&JsonValue>, right: Option<&JsonValue>) -> Ordering {
    match (left, right) {
        (Some(JsonValue::Number(a)), Some(JsonValue::Number(b))) => {
            a.partial_cmp(b).unwrap_or(Ordering::Equal)
        }
        (Some(JsonValue::String(a)), Some(JsonValue::String(b))) => a.cmp(b),
        (Some(JsonValue::Bool(a)), Some(JsonValue::Bool(b))) => a.cmp(b),
        (Some(JsonValue::Null), Some(JsonValue::Null)) => Ordering::Equal,
        (Some(JsonValue::Undefined), Some(JsonValue::Undefined)) => Ordering::Equal,
        (Some(_), Some(_)) => Ordering::Equal,
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
    }
}

fn object_keys_from_value(value: &JsonValue) -> Vec<String> {
    match value {
        JsonValue::Undefined => Vec::new(),
        JsonValue::String(text) => vec![text.clone()],
        JsonValue::Number(num) => vec![num.to_string()],
        JsonValue::Bool(flag) => vec![flag.to_string()],
        JsonValue::Null => vec!["null".to_owned()],
        JsonValue::Array(array) => {
            let mut keys = Vec::new();
            for item in &array.elements {
                keys.extend(object_keys_from_value(item));
            }
            keys
        }
        JsonValue::Object(_) | JsonValue::Function(_) => Vec::new(),
    }
}

fn upsert_object_property(object: &mut JsonObject, key: String, value: JsonValue) {
    for (existing_key, existing_value) in &mut object.0 {
        if *existing_key == key {
            *existing_value = value;
            return;
        }
    }
    object.0.push((key, value));
}

fn materialize_value(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Array(array) => {
            let elements = array.elements.iter().map(materialize_value).collect();
            JsonValue::Array(JsonArray::new(elements, false, array.outer_wrapper))
        }
        JsonValue::Object(JsonObject(entries)) => {
            let mut out = Vec::with_capacity(entries.len());
            for (key, item) in entries {
                out.push((key.clone(), materialize_value(item)));
            }
            JsonValue::Object(JsonObject(out))
        }
        other => other.clone(),
    }
}

fn extract_lambda_arg_name(arg: &Value) -> Option<String> {
    if let Some(raw) = arg.as_str() {
        let name = raw.trim_start_matches('$').to_owned();
        if !name.is_empty() {
            return Some(name);
        }
    }

    if let Some(raw) = arg.get("value").and_then(Value::as_str) {
        let name = raw.trim_start_matches('$').to_owned();
        if !name.is_empty() {
            return Some(name);
        }
    }

    if arg.get("type").and_then(Value::as_str) == Some("path") {
        let first_step = arg
            .get("steps")
            .and_then(Value::as_array)
            .and_then(|steps| steps.first())?;
        return extract_lambda_arg_name(first_step);
    }

    None
}

fn number_cmp(left: &JsonValue, right: &JsonValue, cmp: fn(f64, f64) -> bool) -> Result<JsonValue, Error> {
    let Some(a) = to_number(left) else {
        return Ok(JsonValue::Undefined);
    };
    let Some(b) = to_number(right) else {
        return Ok(JsonValue::Undefined);
    };
    Ok(JsonValue::Bool(cmp(a, b)))
}

fn to_number(value: &JsonValue) -> Option<f64> {
    match value {
        JsonValue::Number(n) => Some(*n),
        JsonValue::Bool(true) => Some(1.0),
        JsonValue::Bool(false) => Some(0.0),
        JsonValue::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn values_equal(left: &JsonValue, right: &JsonValue) -> bool {
    match (left, right) {
        (JsonValue::Undefined, JsonValue::Undefined) => true,
        (JsonValue::Null, JsonValue::Null) => true,
        (JsonValue::Bool(a), JsonValue::Bool(b)) => a == b,
        (JsonValue::Number(a), JsonValue::Number(b)) => a == b,
        (JsonValue::String(a), JsonValue::String(b)) => a == b,
        _ => false,
    }
}

fn is_truthy(value: &JsonValue) -> bool {
    matches!(core::boolean(value), JsonValue::Bool(true))
}

fn to_sequence(value: &JsonValue) -> Vec<JsonValue> {
    match value {
        JsonValue::Undefined => Vec::new(),
        JsonValue::Array(array) => array.elements.clone(),
        other => vec![other.clone()],
    }
}

fn from_sequence(items: Vec<JsonValue>) -> JsonValue {
    match items.len() {
        0 => JsonValue::Undefined,
        1 => items.into_iter().next().unwrap_or(JsonValue::Undefined),
        _ => JsonValue::Array(JsonArray::new(items, true, false)),
    }
}

fn json_value_from_serde(value: &Value) -> JsonValue {
    match value {
        Value::Null => JsonValue::Null,
        Value::Bool(flag) => JsonValue::Bool(*flag),
        Value::Number(num) => JsonValue::Number(num.as_f64().unwrap_or(0.0)),
        Value::String(text) => JsonValue::String(text.clone()),
        Value::Array(values) => JsonValue::Array(JsonArray::new(
            values.iter().map(json_value_from_serde).collect(),
            false,
            false,
        )),
        Value::Object(map) => JsonValue::Object(JsonObject(
            map.iter()
                .map(|(key, item)| (key.clone(), json_value_from_serde(item)))
                .collect(),
        )),
    }
}
