use std::cmp::Ordering;
use std::collections::HashMap;

use futures::future::BoxFuture;
use serde_json::Value;

use crate::error::Error;
use crate::functions::core;
use crate::types::{JsonArray, JsonFunction, JsonObject, JsonValue};

use super::ops::{compare_sort_values, from_sequence, is_truthy, to_sequence};
use super::value::upsert_object_property;
use super::{eval, Bindings};

/// A tuple in a tuple-stream: the current value (`@`) plus a map of ancestor
/// bindings keyed by parent-slot label (e.g. `!0`). Mirrors the object tuples
/// used by upstream JSONata's `evaluateTupleStep`.
#[derive(Clone)]
pub(super) struct Tuple {
    pub value: JsonValue,
    pub bindings: HashMap<String, JsonValue>,
}

pub(super) fn eval_parent(node: &Value, bindings: &Bindings) -> Result<JsonValue, Error> {
    let label = node
        .get("slot")
        .and_then(|slot| slot.get("label"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            Error::new(
                "S0217",
                "The object representing the 'parent' cannot be derived from this expression",
            )
        })?;
    Ok(bindings.get(label).cloned().unwrap_or(JsonValue::Undefined))
}

fn path_has_tuple_step(steps: &[Value]) -> bool {
    steps
        .iter()
        .any(|step| step.get("tuple").and_then(Value::as_bool) == Some(true))
}

pub(super) fn eval_path<'a>(
    node: &'a Value,
    input: &'a JsonValue,
    focus: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    Box::pin(async move {
    let steps = node
        .get("steps")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2002", "Path node missing steps"))?;
    let starts_from_variable = steps
        .first()
        .and_then(|step| step.get("type").and_then(Value::as_str))
        == Some("variable");
    // Mirror upstream `evaluatePath`: build the initial input sequence. A plain
    // array is iterated as-is; anything else (including `undefined`) becomes a
    // singleton sequence so a self-contained first step (e.g. `$split(...)` or a
    // literal `{...}`) still evaluates exactly once. Variable-start paths are
    // absolute and keep the focus verbatim.
    let mut current = if starts_from_variable {
        // Upstream `evaluatePath`: a variable-start (absolute) path always wraps
        // the focus in a singleton sequence, so the leading variable step is
        // evaluated exactly once rather than iterated over the focus elements.
        JsonValue::Array(JsonArray::new(vec![focus.clone()], true, false))
    } else {
        match focus {
            JsonValue::Array(array) if !array.is_sequence => JsonValue::Array(JsonArray::new(
                array.elements.clone(),
                true,
                array.outer_wrapper,
            )),
            JsonValue::Array(_) => focus.clone(),
            other => JsonValue::Array(JsonArray::new(vec![other.clone()], true, false)),
        }
    };

    let mut is_tuple_stream = false;
    let mut tuples: Vec<Tuple> = Vec::new();

    for (index, step) in steps.iter().enumerate() {
        if step.get("tuple").and_then(Value::as_bool) == Some(true) {
            is_tuple_stream = true;
        }

        if index == 0
            && step
                .get("consarray")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        {
            current = eval(step, input, &current, functions, bindings).await?;
            continue;
        }

        if is_tuple_stream {
            tuples = eval_tuple_step(step, input, &current, &tuples, functions, bindings).await?;
            // The implicit input for subsequent non-focus steps is the tuple
            // values; keep `current` in sync for any trailing plain steps.
            current = JsonValue::Array(JsonArray::new(
                tuples.iter().map(|t| t.value.clone()).collect(),
                true,
                false,
            ));
            if tuples.is_empty() {
                break;
            }
        } else {
            current = eval_path_step(
                step,
                index,
                starts_from_variable,
                index == steps.len() - 1,
                input,
                &current,
                functions,
                bindings,
            )
            .await?;
            if let JsonValue::Undefined = current {
                break;
            }
            if let JsonValue::Array(array) = &current {
                if array.elements.is_empty() {
                    break;
                }
            }
        }
    }

    if is_tuple_stream {
        if let Some(group) = node.get("group") {
            // A group expression closing a tuple-stream path reduces tuples that
            // share a key, with their ancestor bindings in scope.
            return eval_group_expression(group, input, tuples, true, functions, bindings).await;
        }
        if node.get("tuple").and_then(Value::as_bool) == Some(true) {
            // Path carries ancestry forward (parenthesised sub-path); leave the
            // tuple stream materialised so the enclosing step keeps the bindings.
            return Ok(tuples_to_carrier(tuples));
        }
        // Return the `@` values as a sequence (not collapsed) so the enclosing
        // `eval` wrapper can honour `keepSingletonArray`/`keepArray` (`[]`) and
        // normalise singletons exactly as upstream `evaluatePath` does.
        let values: Vec<JsonValue> = tuples.into_iter().map(|t| t.value).collect();
        return Ok(JsonValue::Array(JsonArray::new(values, true, false)));
    }

    if let Some(group) = node.get("group") {
        return apply_group_expression(group, input, &current, functions, bindings).await;
    }

    Ok(current)
    })
}

/// Encodes a tuple stream so an enclosing tuple step can recover the carried
/// ancestor bindings. We tag the array as a sequence holding ordinary values
/// but stash the bindings alongside; the enclosing step re-derives them.
fn tuples_to_carrier(tuples: Vec<Tuple>) -> JsonValue {
    // The carrier is just the sequence of values. Ancestor bindings for the
    // inner path are re-applied by the enclosing tuple step, which records its
    // own ancestor; the inner bindings that matter (parent slots) have already
    // been consumed during inner evaluation. To preserve them across a
    // parenthesised boundary we keep them via a side channel.
    TUPLE_CARRIER.with(|cell| {
        *cell.borrow_mut() = Some(tuples.clone());
    });
    JsonValue::Array(JsonArray::new(
        tuples.into_iter().map(|t| t.value).collect(),
        true,
        false,
    ))
}

thread_local! {
    static TUPLE_CARRIER: std::cell::RefCell<Option<Vec<Tuple>>> =
        const { std::cell::RefCell::new(None) };
}

fn take_tuple_carrier() -> Option<Vec<Tuple>> {
    TUPLE_CARRIER.with(|cell| cell.borrow_mut().take())
}

/// Clears any stale tuple carrier left over from a previous evaluation. Called
/// once at the start of each top-level evaluation so the thread-local side
/// channel cannot leak ancestry tuples between independent runs.
pub(super) fn clear_tuple_carrier() {
    TUPLE_CARRIER.with(|cell| *cell.borrow_mut() = None);
}

/// Evaluates one step of a tuple stream, producing the next set of tuples.
/// Each input tuple's bindings are injected into the environment so that
/// `parent` nodes can resolve their slot labels.
fn eval_tuple_step<'a>(
    step: &'a Value,
    input: &'a JsonValue,
    current: &'a JsonValue,
    in_tuples: &'a [Tuple],
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<Vec<Tuple>, Error>> {
    Box::pin(async move {
    // A `sort` step within a tuple stream sorts the stream (or seeds it from the
    // input when there are no tuples yet), mirroring upstream `evaluateTupleStep`.
    if step.get("type").and_then(Value::as_str) == Some("sort") {
        let mut out: Vec<Tuple> = if !in_tuples.is_empty() {
            sort_tuple_stream(step, input, in_tuples, functions, bindings).await?
        } else {
            let sorted = apply_sort_step(step, input, current, functions, bindings).await?;
            let index_key = step.get("index").and_then(Value::as_str);
            to_sequence(&sorted)
                .into_iter()
                .enumerate()
                .map(|(idx, value)| {
                    let mut b = HashMap::new();
                    if let Some(ik) = index_key {
                        b.insert(ik.to_string(), JsonValue::Number(idx as f64));
                    }
                    Tuple { value, bindings: b }
                })
                .collect()
        };
        if let Some(stages) = step.get("stages").and_then(Value::as_array) {
            for stage in stages {
                out = apply_tuple_stage(stage, input, out, functions, bindings).await?;
            }
        }
        return Ok(out);
    }

    // Seed the tuple stream from the current sequence on the first tuple step.
    let seed: Vec<Tuple> = if in_tuples.is_empty() {
        to_sequence(current)
            .into_iter()
            .map(|value| Tuple {
                value,
                bindings: HashMap::new(),
            })
            .collect()
    } else {
        in_tuples.to_vec()
    };

    let focus_key = step.get("focus").and_then(Value::as_str);
    let index_key = step.get("index").and_then(Value::as_str);
    let ancestor_label = step
        .get("ancestor")
        .and_then(|a| a.get("label"))
        .and_then(Value::as_str);

    // A `predicate` on a tuple step (e.g. a parenthesised tuple-producing block
    // followed by `[...]`) must be applied AFTER the tuple stream — with each
    // tuple's ancestor bindings in scope — so `%` (parent) resolves correctly.
    // Strip it from the step we pass to `eval` (otherwise the generic `eval`
    // wrapper would apply it eagerly, losing ancestry) and re-apply it below as
    // a sequence of filter stages.
    let step_predicate = step.get("predicate").and_then(Value::as_array);
    let stripped_step;
    let eval_step: &Value = if step_predicate.is_some() {
        let mut copy = step.clone();
        if let Some(obj) = copy.as_object_mut() {
            obj.remove("predicate");
        }
        stripped_step = copy;
        &stripped_step
    } else {
        step
    };

    let mut out: Vec<Tuple> = Vec::new();
    for tuple in &seed {
        // Build an environment with the tuple's ancestor bindings in scope.
        let mut local = bindings.clone();
        for (k, v) in &tuple.bindings {
            local.insert(k.clone(), v.clone());
        }

        // Clear any stale carrier so only a tuple stream produced by *this*
        // step's evaluation is observed (the carrier is a thread-local side
        // channel and could otherwise leak across evaluations).
        let _ = take_tuple_carrier();
        let res = eval(eval_step, input, &tuple.value, functions, &local).await?;
        if res.is_undefined() {
            continue;
        }

        // If the inner step carried a tuple stream (parenthesised sub-path that
        // itself tracks ancestry), merge those tuple bindings in.
        if let Some(inner_tuples) = take_tuple_carrier() {
            for inner in inner_tuples {
                let mut merged = tuple.bindings.clone();
                for (k, v) in inner.bindings {
                    merged.insert(k, v);
                }
                if let Some(label) = ancestor_label {
                    merged.insert(label.to_string(), tuple.value.clone());
                }
                out.push(Tuple {
                    value: inner.value,
                    bindings: merged,
                });
            }
            continue;
        }

        // Upstream `evaluateTupleStep` iterates any array result (Array.isArray),
        // not just sequences, producing one output tuple per element.
        let produced = match res {
            JsonValue::Array(array) => array.elements,
            other => vec![other],
        };

        for (bb, val) in produced.into_iter().enumerate() {
            let mut merged = tuple.bindings.clone();
            let mut value = val;
            if let Some(fk) = focus_key {
                merged.insert(fk.to_string(), value.clone());
                value = tuple.value.clone();
            }
            if let Some(ik) = index_key {
                merged.insert(ik.to_string(), JsonValue::Number(bb as f64));
            }
            if let Some(label) = ancestor_label {
                merged.insert(label.to_string(), tuple.value.clone());
            }
            out.push(Tuple {
                value,
                bindings: merged,
            });
        }
    }

    // Apply this step's stages (filters / index) to the produced tuple stream.
    // Upstream `evaluateTupleStep` runs `evaluateStages` after building the
    // stream, evaluating each filter per tuple with the tuple's ancestor
    // bindings in scope so that `%` resolves to the correct parent.
    if let Some(stages) = step.get("stages").and_then(Value::as_array) {
        for stage in stages {
            out = apply_tuple_stage(stage, input, out, functions, bindings).await?;
        }
    }

    // Apply the stripped predicate (if any) as filter stages over the tuple
    // stream so ancestor bindings (`%`) are in scope for each predicate.
    if let Some(predicates) = step_predicate {
        for predicate in predicates {
            out = apply_tuple_stage(predicate, input, out, functions, bindings).await?;
        }
    }

    Ok(out)
    })
}

/// Applies a single stage (filter / index) to a tuple stream, evaluating filter
/// predicates against each tuple's `@` value with its ancestor bindings in
/// scope. Mirrors `evaluateStages` + `evaluateFilter` for the tupleStream case.
fn apply_tuple_stage<'a>(
    stage: &'a Value,
    input: &'a JsonValue,
    tuples: Vec<Tuple>,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<Vec<Tuple>, Error>> {
    Box::pin(async move {
    let stage_type = stage
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match stage_type {
        "filter" => {
            let expr = stage
                .get("expr")
                .ok_or_else(|| Error::new("E2004", "Filter stage missing expr"))?;
            let len = tuples.len() as i64;

            // Literal numeric predicate selects a single positional tuple.
            if expr.get("type").and_then(Value::as_str) == Some("number") {
                if let Some(raw) = expr.get("value").and_then(Value::as_f64) {
                    let mut idx = raw.floor() as i64;
                    if idx < 0 {
                        idx += len;
                    }
                    if idx < 0 || idx >= len {
                        return Ok(Vec::new());
                    }
                    return Ok(vec![tuples.into_iter().nth(idx as usize).unwrap()]);
                }
            }

            let mut kept: Vec<Tuple> = Vec::new();
            for (index, tuple) in tuples.iter().enumerate() {
                let mut local = bindings.clone();
                for (k, v) in &tuple.bindings {
                    local.insert(k.clone(), v.clone());
                }
                let res = eval(expr, input, &tuple.value, functions, &local).await?;
                if let Some(numbers) = numeric_indices(&res) {
                    for raw in numbers {
                        let mut ii = raw.floor() as i64;
                        if ii < 0 {
                            ii += len;
                        }
                        if ii == index as i64 {
                            kept.push(tuple.clone());
                        }
                    }
                } else if is_truthy(&res) {
                    kept.push(tuple.clone());
                }
            }
            Ok(kept)
        }
        "index" => {
            let key = stage.get("value").and_then(Value::as_str);
            let mut out = tuples;
            if let Some(key) = key {
                for (ee, tuple) in out.iter_mut().enumerate() {
                    tuple
                        .bindings
                        .insert(key.to_string(), JsonValue::Number(ee as f64));
                }
            }
            Ok(out)
        }
        other => Err(Error::new(
            "E2005",
            format!("Unsupported stage type: {other}"),
        )),
    }
    })
}

fn eval_path_step<'a>(
    step: &'a Value,
    _step_index: usize,
    _starts_from_variable: bool,
    is_last: bool,
    input: &'a JsonValue,
    current: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    Box::pin(async move {
    let step_type = step.get("type").and_then(Value::as_str).unwrap_or_default();
    let stages = step.get("stages").and_then(Value::as_array);

    // Sort steps operate on the whole input sequence at once (upstream
    // `evaluateStep` short-circuits the `sort` type before per-item iteration).
    if step_type == "sort" {
        let mut out = apply_sort_step(step, input, current, functions, bindings).await?;
        if let Some(stages) = stages {
            for stage in stages {
                out = apply_stage(stage, input, &out, functions, bindings).await?;
            }
        }
        return Ok(out);
    }

    // Mirror upstream `evaluateStep`: iterate every item of the input sequence,
    // evaluate the step expression against that item, apply this step's stages
    // to each individual result, then flatten the per-item results into one
    // sequence (cons arrays / non-sequence arrays are NOT flattened).
    let mut results: Vec<JsonValue> = Vec::new();
    for item in to_sequence(current) {
        let mut res = eval_step_expr(step, step_type, input, &item, functions, bindings).await?;
        if let Some(stages) = stages {
            for stage in stages {
                res = apply_stage(stage, input, &res, functions, bindings).await?;
            }
        }
        if !res.is_undefined() {
            results.push(res);
        }
    }

    // Upstream last-step rule: if a single result that is a plain (non-sequence)
    // array survives, return it as-is rather than wrapping/flattening it.
    if is_last
        && results.len() == 1
        && matches!(&results[0], JsonValue::Array(array) if !array.is_sequence)
    {
        let mut out = results.into_iter().next().unwrap_or(JsonValue::Undefined);
        if let Some(index) = step.get("index") {
            out = apply_index(&out, index);
        }
        return Ok(out);
    }

    let mut flattened: Vec<JsonValue> = Vec::new();
    for res in results {
        match res {
            // Upstream `evaluateStep` flattens every array result EXCEPT cons
            // arrays (explicit array constructors, marked here by `outer_wrapper`).
            // Plain input arrays and sequences are both flattened one level.
            JsonValue::Array(array) if !array.outer_wrapper => {
                flattened.extend(array.elements);
            }
            other => flattened.push(other),
        }
    }

    let mut out = JsonValue::Array(JsonArray::new(flattened, true, false));

    if let Some(index) = step.get("index") {
        out = apply_index(&out, index);
    }

    Ok(out)
    })
}

/// Evaluates a single step expression against one input item, matching the
/// `evaluate(expr, input[ii], environment)` call inside upstream `evaluateStep`.
fn eval_step_expr<'a>(
    step: &'a Value,
    step_type: &'a str,
    input: &'a JsonValue,
    item: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    Box::pin(async move {
    // A step carrying a `predicate` or `group` must be routed through `eval`
    // (rather than the name/wildcard/descendant fast paths) so those are applied
    // per item, matching upstream `evaluateStep`'s `evaluate(step, input[ii])`.
    if step.get("predicate").is_some() || step.get("group").is_some() {
        return eval(step, input, item, functions, bindings).await;
    }
    match step_type {
        "name" => {
            let key = step
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Ok(core::lookup(item, key))
        }
        "wildcard" => Ok(apply_wildcard(item)),
        "descendant" => Ok(apply_descendant(item)),
        _ => eval(step, input, item, functions, bindings).await,
    }
    })
}

fn apply_stage<'a>(
    stage: &'a Value,
    input: &'a JsonValue,
    current: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    Box::pin(async move {
    let stage_type = stage
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();

    match stage_type {
        "filter" => {
            let expr = stage
                .get("expr")
                .ok_or_else(|| Error::new("E2004", "Filter stage missing expr"))?;
            evaluate_filter(expr, input, current, functions, bindings).await
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
    })
}

pub(super) fn apply_predicate_expr<'a>(
    expr: &'a Value,
    input: &'a JsonValue,
    current: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    evaluate_filter(expr, input, current, functions, bindings)
}

/// Mirrors upstream `evaluateFilter`: applies a predicate/filter expression to
/// an input sequence. A literal numeric predicate selects a single index
/// (negative counts from the end, fractional floors). Otherwise the predicate
/// is evaluated per item; numeric (or array-of-number) results are treated as
/// positional indices, while non-numeric results filter by truthiness.
fn evaluate_filter<'a>(
    expr: &'a Value,
    input: &'a JsonValue,
    current: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    Box::pin(async move {
    let items: Vec<JsonValue> = to_sequence(current);
    let len = items.len() as i64;

    // Fast path: literal numeric predicate selects a single positional index.
    if expr.get("type").and_then(Value::as_str) == Some("number") {
        if let Some(raw) = expr.get("value").and_then(Value::as_f64) {
            let mut index = raw.floor() as i64;
            if index < 0 {
                index += len;
            }
            if index < 0 || index >= len {
                return Ok(JsonValue::Undefined);
            }
            // Upstream `evaluateFilter`: if the selected item is itself an array,
            // it becomes the whole result (returned as-is); otherwise it is
            // pushed into a result sequence.
            let item = items[index as usize].clone();
            return Ok(match item {
                JsonValue::Array(_) => item,
                other => JsonValue::Array(JsonArray::new(vec![other], true, false)),
            });
        }
    }

    let mut kept: Vec<JsonValue> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let res = eval(expr, input, item, functions, bindings).await?;
        let numbers = numeric_indices(&res);
        if let Some(numbers) = numbers {
            for raw in numbers {
                let mut ii = raw.floor() as i64;
                if ii < 0 {
                    ii += len;
                }
                if ii == index as i64 {
                    kept.push(item.clone());
                }
            }
        } else if is_truthy(&res) {
            kept.push(item.clone());
        }
    }
    // Upstream `evaluateFilter` always returns a sequence; the singleton-collapse
    // is performed later by `evaluate`/`normalize_sequence` (unless keepArray).
    if kept.is_empty() {
        return Ok(JsonValue::Undefined);
    }
    Ok(JsonValue::Array(JsonArray::new(kept, true, false)))
    })
}

/// Returns the numeric values of `value` if it is a number or an array whose
/// elements are all numbers (matching upstream `isNumeric` / `isArrayOfNumbers`).
fn numeric_indices(value: &JsonValue) -> Option<Vec<f64>> {
    match value {
        JsonValue::Number(num) if num.is_finite() => Some(vec![*num]),
        JsonValue::Array(array) => {
            if array.elements.is_empty() {
                return None;
            }
            let mut numbers = Vec::with_capacity(array.elements.len());
            for element in &array.elements {
                match element {
                    JsonValue::Number(num) if num.is_finite() => numbers.push(*num),
                    _ => return None,
                }
            }
            Some(numbers)
        }
        _ => None,
    }
}

#[allow(dead_code)]
fn eval_path_expr_step<'a>(
    step: &'a Value,
    input: &'a JsonValue,
    current: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
    applyto_context: bool,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    Box::pin(async move {
    let mut values = to_sequence(current);
    if values.is_empty() && current.is_undefined() {
        values.push(JsonValue::Undefined);
    }
    if values.is_empty() {
        return Ok(JsonValue::Undefined);
    }

    let mut out = Vec::new();
    for item in values {
        let value = if applyto_context && step.get("type").and_then(Value::as_str) == Some("function") {
            super::callable::eval_function_with_applyto(
                step, input, &item, functions, bindings, &item,
            )
            .await?
        } else {
            eval(step, input, &item, functions, bindings).await?
        };
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
    })
}

pub(super) fn apply_group_expression<'a>(
    group: &'a Value,
    input: &'a JsonValue,
    current: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    Box::pin(async move {
    let items: Vec<Tuple> = to_sequence(current)
        .into_iter()
        .map(|value| Tuple {
            value,
            bindings: HashMap::new(),
        })
        .collect();
    eval_group_expression(group, input, items, false, functions, bindings).await
    })
}

/// Evaluates `{...}` group-by construction, mirroring upstream
/// `evaluateGroupExpression`. When `reduce` is true the inputs are tuples (a
/// tuple stream) whose ancestor bindings (and `@` value) are brought into scope
/// for both the key and value expressions; grouped tuples are reduced together.
pub(super) fn eval_group_expression<'a>(
    group: &'a Value,
    input: &'a JsonValue,
    mut items: Vec<Tuple>,
    reduce: bool,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    Box::pin(async move {
    let pairs = group
        .get("lhs")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2030", "Group expression missing lhs"))?;

    // An empty input still allows a literal object to be generated.
    if items.is_empty() {
        items.push(Tuple {
            value: JsonValue::Undefined,
            bindings: HashMap::new(),
        });
    }

    // Group the input by key, recording which pair (expression) produced each
    // group so duplicate keys from different expressions can be flagged (D1009).
    struct Group {
        tuples: Vec<Tuple>,
        expr_index: usize,
    }
    let mut groups: HashMap<String, Group> = HashMap::new();
    let mut key_order: Vec<String> = Vec::new();

    for item in &items {
        let mut env = bindings.clone();
        if reduce {
            for (k, v) in &item.bindings {
                env.insert(k.clone(), v.clone());
            }
        }
        for (pair_index, pair) in pairs.iter().enumerate() {
            let pair_values = pair
                .as_array()
                .ok_or_else(|| Error::new("E2031", "Group pair must be array"))?;
            if pair_values.len() != 2 {
                return Err(Error::new("E2032", "Group pair must contain key and value"));
            }
            let key_expr = &pair_values[0];
            let key_value = eval(key_expr, input, &item.value, functions, &env).await?;
            let key = match key_value {
                JsonValue::String(text) => text,
                JsonValue::Undefined => continue,
                _ => return Err(Error::new("T1003", "Key in object structure must evaluate to a string")),
            };

            match groups.get_mut(&key) {
                Some(existing) => {
                    if existing.expr_index != pair_index {
                        return Err(Error::new(
                            "D1009",
                            "Multiple key definitions evaluate to same key in object constructor",
                        ));
                    }
                    existing.tuples.push(item.clone());
                }
                None => {
                    key_order.push(key.clone());
                    groups.insert(
                        key,
                        Group {
                            tuples: vec![item.clone()],
                            expr_index: pair_index,
                        },
                    );
                }
            }
        }
    }

    let mut result = JsonObject(Vec::new());
    for key in key_order {
        let group_entry = groups.remove(&key).unwrap();
        let pair_values = pairs[group_entry.expr_index].as_array().unwrap();
        let value_expr = &pair_values[1];

        let (context, env) = if reduce {
            let (reduced_value, reduced_bindings) = reduce_tuple_stream(&group_entry.tuples);
            let mut env = bindings.clone();
            for (k, v) in reduced_bindings {
                env.insert(k, v);
            }
            (reduced_value, env)
        } else {
            let values: Vec<JsonValue> =
                group_entry.tuples.into_iter().map(|t| t.value).collect();
            (
                JsonValue::Array(JsonArray::new(values, true, false)),
                bindings.clone(),
            )
        };

        let value = eval(value_expr, input, &context, functions, &env).await?;
        if !value.is_undefined() {
            upsert_object_property(&mut result, key, value);
        }
    }

    Ok(JsonValue::Object(result))
    })
}

/// Reduces a group of tuples into a single `@` context value plus a merged set
/// of ancestor bindings (mirrors upstream `reduceTupleStream`). When several
/// tuples share a binding label the values are appended.
fn reduce_tuple_stream(tuples: &[Tuple]) -> (JsonValue, HashMap<String, JsonValue>) {
    let mut value = JsonValue::Undefined;
    let mut merged: HashMap<String, JsonValue> = HashMap::new();
    for (i, tuple) in tuples.iter().enumerate() {
        if i == 0 {
            value = tuple.value.clone();
            merged = tuple.bindings.clone();
        } else {
            value = core::append(&value, &tuple.value);
            for (k, v) in &tuple.bindings {
                let combined = match merged.get(k) {
                    Some(existing) => core::append(existing, v),
                    None => v.clone(),
                };
                merged.insert(k.clone(), combined);
            }
        }
    }
    (value, merged)
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
    let Some(idx) = numeric_index(Some(index)) else {
        return JsonValue::Undefined;
    };

    let items: Vec<JsonValue> = match current {
        JsonValue::Array(array) if !array.is_sequence => array.elements.clone(),
        _ => to_sequence(current),
    };
    if items.is_empty() {
        return JsonValue::Undefined;
    }

    let position = if idx < 0 {
        let from_end = items.len() as i64 + idx;
        if from_end < 0 {
            return JsonValue::Undefined;
        }
        from_end as usize
    } else {
        idx as usize
    };

    items
        .get(position)
        .cloned()
        .unwrap_or(JsonValue::Undefined)
}

pub(super) fn apply_sort_step<'a>(
    step: &'a Value,
    input: &'a JsonValue,
    current: &'a JsonValue,
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<JsonValue, Error>> {
    Box::pin(async move {
    let terms = step
        .get("terms")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2029", "Sort step missing terms"))?;

    let mut values = to_sequence(current);

    // Stable insertion sort so we can surface comparator errors. The comparison
    // mirrors the reference `evaluateSortExpression` comparator, including the
    // type checks (T2007/T2008) and undefined-last ordering.
    let len = values.len();
    for i in 1..len {
        let mut j = i;
        while j > 0 {
            let ordering = compare_sort_terms(
                terms,
                input,
                &values[j - 1],
                &values[j],
                functions,
                bindings,
            )
            .await?;
            match ordering {
                Ordering::Greater => {
                    values.swap(j - 1, j);
                    j -= 1;
                }
                _ => break,
            }
        }
    }

    Ok(from_sequence(values))
    })
}

/// Compares two values using each order-by term in priority order, mirroring
/// the reference `evaluateSortExpression` comparator (T2007/T2008 type checks,
/// undefined-last ordering).
async fn compare_sort_terms(
    terms: &[Value],
    input: &JsonValue,
    left: &JsonValue,
    right: &JsonValue,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<Ordering, Error> {
    for term in terms {
        let descending = term
            .get("descending")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let expr = match term.get("expression") {
            Some(expr) => expr,
            None => continue,
        };

        let aa = eval(expr, input, left, functions, bindings).await?;
        let bb = eval(expr, input, right, functions, bindings).await?;

        if let Some(ordering) = compare_sort_term_values(&aa, &bb, descending)? {
            return Ok(ordering);
        }
    }
    Ok(Ordering::Equal)
}

/// Applies the per-term comparison rules to two already-evaluated key values,
/// returning `None` when the keys are equal (so the next term is consulted).
fn compare_sort_term_values(
    aa: &JsonValue,
    bb: &JsonValue,
    descending: bool,
) -> Result<Option<Ordering>, Error> {
    let a_missing = aa.is_undefined();
    let b_missing = bb.is_undefined();
    if a_missing {
        if b_missing {
            return Ok(None);
        }
        return Ok(Some(Ordering::Greater));
    }
    if b_missing {
        return Ok(Some(Ordering::Less));
    }

    let a_ok = matches!(aa, JsonValue::String(_) | JsonValue::Number(_));
    let b_ok = matches!(bb, JsonValue::String(_) | JsonValue::Number(_));
    if !a_ok || !b_ok {
        return Err(Error::new(
            "T2008",
            "The expressions within an order-by clause must evaluate to numeric or string values",
        ));
    }
    let same_type = matches!(
        (aa, bb),
        (JsonValue::String(_), JsonValue::String(_))
            | (JsonValue::Number(_), JsonValue::Number(_))
    );
    if !same_type {
        return Err(Error::new(
            "T2007",
            "Type mismatch when comparing values in order-by clause",
        ));
    }

    let mut ordering = compare_sort_values(Some(aa), Some(bb));
    if descending {
        ordering = ordering.reverse();
    }
    if ordering != Ordering::Equal {
        return Ok(Some(ordering));
    }
    Ok(None)
}

/// Sorts a tuple stream by the sort terms, evaluating each term against the
/// tuple's `@` value with its ancestor bindings in scope. Mirrors upstream
/// `evaluateSortExpression` invoked with a `tupleStream`.
fn sort_tuple_stream<'a>(
    step: &'a Value,
    input: &'a JsonValue,
    tuples: &'a [Tuple],
    functions: &'a HashMap<String, JsonFunction>,
    bindings: &'a Bindings,
) -> BoxFuture<'a, Result<Vec<Tuple>, Error>> {
    Box::pin(async move {
    let terms = step
        .get("terms")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("E2029", "Sort step missing terms"))?;

    let mut items: Vec<Tuple> = tuples.to_vec();

    let len = items.len();
    for i in 1..len {
        let mut j = i;
        while j > 0 {
            let ordering =
                compare_tuple_terms(terms, input, &items[j - 1], &items[j], functions, bindings)
                    .await?;
            match ordering {
                Ordering::Greater => {
                    items.swap(j - 1, j);
                    j -= 1;
                }
                _ => break,
            }
        }
    }

    Ok(items)
    })
}

/// Compares two tuples by the sort terms, evaluating each term against the
/// tuple's `@` value with its ancestor bindings in scope.
async fn compare_tuple_terms(
    terms: &[Value],
    input: &JsonValue,
    left: &Tuple,
    right: &Tuple,
    functions: &HashMap<String, JsonFunction>,
    bindings: &Bindings,
) -> Result<Ordering, Error> {
    let mut env_l = bindings.clone();
    for (k, v) in &left.bindings {
        env_l.insert(k.clone(), v.clone());
    }
    let mut env_r = bindings.clone();
    for (k, v) in &right.bindings {
        env_r.insert(k.clone(), v.clone());
    }
    for term in terms {
        let descending = term
            .get("descending")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let expr = match term.get("expression") {
            Some(expr) => expr,
            None => continue,
        };
        let aa = eval(expr, input, &left.value, functions, &env_l).await?;
        let bb = eval(expr, input, &right.value, functions, &env_r).await?;

        if let Some(ordering) = compare_sort_term_values(&aa, &bb, descending)? {
            return Ok(ordering);
        }
    }
    Ok(Ordering::Equal)
}

pub(super) fn apply_wildcard(current: &JsonValue) -> JsonValue {
    // Mirror upstream `evaluateWildcard`: an outer-wrapper array unwraps to its
    // first element before selecting; object values that are arrays are deeply
    // flattened into the result sequence; other values are pushed verbatim.
    let target = match current {
        JsonValue::Array(array) if array.outer_wrapper && !array.elements.is_empty() => {
            &array.elements[0]
        }
        other => other,
    };

    // Upstream iterates `Object.keys(input)`; JS arrays are objects too, so a
    // plain array yields its elements. The per-value array-flatten rule applies
    // in both cases.
    let member_values: Vec<&JsonValue> = match target {
        JsonValue::Object(JsonObject(entries)) => entries.iter().map(|(_, value)| value).collect(),
        JsonValue::Array(array) => array.elements.iter().collect(),
        _ => return JsonValue::Undefined,
    };

    let mut values: Vec<JsonValue> = Vec::new();
    for value in member_values {
        match value {
            JsonValue::Array(_) => flatten_deep(value, &mut values),
            other => values.push(other.clone()),
        }
    }
    from_sequence(values)
}

/// Recursively flattens nested arrays into `out`, matching upstream `flatten`.
fn flatten_deep(value: &JsonValue, out: &mut Vec<JsonValue>) {
    match value {
        JsonValue::Array(array) => {
            for element in &array.elements {
                flatten_deep(element, out);
            }
        }
        other => out.push(other.clone()),
    }
}

pub(super) fn apply_descendant(current: &JsonValue) -> JsonValue {
    if current.is_undefined() {
        return JsonValue::Undefined;
    }
    let mut out = Vec::new();
    collect_descendants(current, &mut out);
    from_sequence(out)
}

/// Mirrors upstream `recurseDescendants`: every non-array value (objects and
/// scalars) is collected; arrays are transparent (only their members appear,
/// never the array itself); object values are recursed into.
fn collect_descendants(value: &JsonValue, out: &mut Vec<JsonValue>) {
    match value {
        JsonValue::Array(array) => {
            for item in &array.elements {
                collect_descendants(item, out);
            }
        }
        JsonValue::Object(JsonObject(entries)) => {
            out.push(value.clone());
            for (_, entry) in entries {
                collect_descendants(entry, out);
            }
        }
        other => out.push(other.clone()),
    }
}

