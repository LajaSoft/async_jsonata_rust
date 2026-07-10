use regex::RegexBuilder;
use serde_json;

use crate::types::{FunctionContext, JsonArray, JsonError, JsonFunction, JsonObject, JsonValue};

fn get_object_property<'a>(object: &'a JsonObject, key: &str) -> Option<&'a JsonValue> {
    object.0.iter().find_map(|(name, value)| {
        if name == key {
            return Some(value);
        }
        None
    })
}

fn value_json(value: &JsonValue) -> String {
    match value {
        JsonValue::Undefined => "null".to_owned(),
        JsonValue::Null => "null".to_owned(),
        JsonValue::Bool(flag) => flag.to_string(),
        JsonValue::Number(num) => num.to_string(),
        JsonValue::String(text) => serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned()),
        _ => "\"[complex]\"".to_owned(),
    }
}

fn matcher_result_to_object(match_text: String, start: f64, groups: Vec<JsonValue>) -> JsonValue {
    JsonValue::Object(JsonObject(vec![
        ("match".to_owned(), JsonValue::String(match_text)),
        ("index".to_owned(), JsonValue::Number(start)),
        (
            "groups".to_owned(),
            JsonValue::Array(JsonArray::new(groups, false, false)),
        ),
    ]))
}

#[derive(Clone, Debug)]
struct MatcherResult {
    match_text: String,
    start: usize,
    end: usize,
    groups: Vec<Option<String>>,
    next: Option<JsonFunction>,
}

impl MatcherResult {
    fn callback_object(&self) -> JsonValue {
        let groups = self
            .groups
            .iter()
            .map(|group| match group {
                Some(value) => JsonValue::String(value.clone()),
                None => JsonValue::Undefined,
            })
            .collect::<Vec<JsonValue>>();
        let mut props = vec![
            ("match".to_owned(), JsonValue::String(self.match_text.clone())),
            ("start".to_owned(), JsonValue::Number(self.start as f64)),
            ("end".to_owned(), JsonValue::Number(self.end as f64)),
            ("index".to_owned(), JsonValue::Number(self.start as f64)),
            (
                "groups".to_owned(),
                JsonValue::Array(JsonArray::new(groups, false, false)),
            ),
        ];
        if let Some(next) = &self.next {
            props.push(("next".to_owned(), JsonValue::Function(next.clone())));
        }
        JsonValue::Object(JsonObject(props))
    }
}

fn matcher_result_from_json(value: JsonValue) -> std::result::Result<Option<MatcherResult>, JsonError> {
    match value {
        JsonValue::Undefined | JsonValue::Null => Ok(None),
        JsonValue::Object(object) => {
            let has_start = matches!(
                get_object_property(&object, "start"),
                Some(JsonValue::Number(_))
            );
            let has_end = matches!(get_object_property(&object, "end"), Some(JsonValue::Number(_)));
            let has_groups = matches!(
                get_object_property(&object, "groups"),
                Some(JsonValue::Array(_))
            );
            let has_next = matches!(
                get_object_property(&object, "next"),
                Some(JsonValue::Function(_))
            );

            if !(has_start || has_end || has_groups || has_next) {
                return Err(JsonError::new(
                    "T1010",
                    "Matcher function returned unsupported structure",
                ));
            }

            let match_text = match get_object_property(&object, "match") {
                Some(JsonValue::String(text)) => text.clone(),
                Some(JsonValue::Undefined) | None => String::new(),
                Some(other) => {
                    return Err(JsonError::new(
                        "T1010",
                        format!("Matcher result field 'match' must be string, got {:?}", other),
                    ))
                }
            };

            let start = match get_object_property(&object, "start") {
                Some(JsonValue::Number(num)) if num.is_finite() && *num >= 0.0 => *num as usize,
                Some(JsonValue::Undefined) | None => 0usize,
                Some(other) => {
                    return Err(JsonError::new(
                        "T1010",
                        format!("Matcher result field 'start' must be number, got {:?}", other),
                    ))
                }
            };

            let end = match get_object_property(&object, "end") {
                Some(JsonValue::Number(num)) if num.is_finite() && *num >= 0.0 => *num as usize,
                Some(JsonValue::Undefined) | None => start.saturating_add(match_text.len()),
                Some(other) => {
                    return Err(JsonError::new(
                        "T1010",
                        format!("Matcher result field 'end' must be number, got {:?}", other),
                    ))
                }
            };

            let groups = match get_object_property(&object, "groups") {
                Some(JsonValue::Array(array)) => array
                    .elements
                    .iter()
                    .map(|value| match value {
                        JsonValue::Undefined => Ok(None),
                        JsonValue::String(text) => Ok(Some(text.clone())),
                        other => Err(JsonError::new(
                            "T1010",
                            format!("Matcher result group must be string, got {:?}", other),
                        )),
                    })
                    .collect::<std::result::Result<Vec<Option<String>>, JsonError>>()?,
                Some(JsonValue::Undefined) | None => Vec::new(),
                Some(other) => {
                    return Err(JsonError::new(
                        "T1010",
                        format!("Matcher result field 'groups' must be array, got {:?}", other),
                    ))
                }
            };

            let next = match get_object_property(&object, "next") {
                Some(JsonValue::Function(function)) => Some(function.clone()),
                _ => None,
            };

            Ok(Some(MatcherResult {
                match_text,
                start,
                end,
                groups,
                next,
            }))
        }
        _ => Err(JsonError::new(
            "T1010",
            "Matcher function returned unsupported structure",
        )),
    }
}

async fn evaluate_matcher(
    focus: FunctionContext,
    matcher: &JsonFunction,
    input: Option<String>,
) -> std::result::Result<Option<MatcherResult>, JsonError> {
    let mut args = Vec::with_capacity(1);
    match input {
        Some(text) => args.push(JsonValue::String(text)),
        None => args.push(JsonValue::Undefined),
    }
    // A user-supplied matcher's `next` closure is typically `function(){ … }`
    // whose body is a tail call, i.e. a thunk. Drive it to a concrete match
    // object rather than handing the raw thunk to `matcher_result_from_json`.
    let result = matcher.call_forced(focus, args).await?;
    matcher_result_from_json(result)
}

fn replacement_string_from_match(replacement: &str, regex_match: &MatcherResult) -> String {
    let mut substitute = String::new();
    let mut position = 0usize;

    while position < replacement.len() {
        let next_dollar = replacement[position..].find('$');
        let Some(relative_index) = next_dollar else {
            substitute.push_str(&replacement[position..]);
            break;
        };

        let dollar_index = position + relative_index;
        substitute.push_str(&replacement[position..dollar_index]);
        position = dollar_index + 1;

        if position >= replacement.len() {
            substitute.push('$');
            break;
        }

        let next_char = replacement.as_bytes()[position] as char;
        if next_char == '$' {
            substitute.push('$');
            position += 1;
            continue;
        }
        if next_char == '0' {
            substitute.push_str(&regex_match.match_text);
            position += 1;
            continue;
        }

        let max_digits = if regex_match.groups.is_empty() {
            1usize
        } else {
            ((regex_match.groups.len() as f64).log10().floor() as usize) + 1
        };
        let remaining = replacement.len() - position;
        let first_len = max_digits.min(remaining);
        let first_slice = &replacement[position..position + first_len];
        let first_digits_len = first_slice
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .count();
        let mut capture_index = if first_digits_len == 0 {
            None
        } else {
            first_slice[..first_digits_len].parse::<usize>().ok()
        };
        if max_digits > 1 {
            if let Some(parsed) = capture_index {
                if parsed > regex_match.groups.len() && first_len > 1 {
                    let second_slice = &replacement[position..position + first_len - 1];
                    let second_digits_len = second_slice
                        .chars()
                        .take_while(|ch| ch.is_ascii_digit())
                        .count();
                    capture_index = if second_digits_len == 0 {
                        None
                    } else {
                        second_slice[..second_digits_len].parse::<usize>().ok()
                    };
                }
            }
        }

        if let Some(index) = capture_index {
            if !regex_match.groups.is_empty() && index > 0 {
                if let Some(Some(group)) = regex_match.groups.get(index - 1) {
                    substitute.push_str(group);
                }
            }
            position += index.to_string().len();
            continue;
        }

        substitute.push('$');
    }

    substitute
}

fn regex_pattern_from_matcher(value: &JsonValue) -> Option<(String, String)> {
    let object = match value {
        JsonValue::Object(obj) => obj,
        _ => return None,
    };
    let source = match get_object_property(object, "__jsonata_regex_source") {
        Some(JsonValue::String(value)) => value.clone(),
        _ => return None,
    };
    let flags = match get_object_property(object, "__jsonata_regex_flags") {
        Some(JsonValue::String(value)) => value.clone(),
        _ => String::new(),
    };
    Some((source, flags))
}

fn build_rust_regex(source: &str, flags: &str) -> std::result::Result<regex::Regex, JsonError> {
    let mut builder = RegexBuilder::new(source);
    for flag in flags.chars() {
        match flag {
            'i' => {
                builder.case_insensitive(true);
            }
            'm' => {
                builder.multi_line(true);
            }
            's' => {
                builder.dot_matches_new_line(true);
            }
            'x' => {
                builder.ignore_whitespace(true);
            }
            'u' | 'g' | 'y' => {}
            _ => {}
        }
    }
    builder
        .build()
        .map_err(|err| JsonError::new("T1010", format!("Invalid regex: {err}")))
}

pub async fn match_function(
    focus: FunctionContext,
    input: JsonValue,
    matcher: JsonValue,
    limit: JsonValue,
) -> std::result::Result<JsonValue, JsonError> {
    if matches!(input, JsonValue::Undefined) {
        return Ok(JsonValue::Undefined);
    }

    let input_text = match input {
        JsonValue::String(text) => text,
        other => {
            return Err(JsonError::new(
                "T0410",
                format!(
                    "Argument 1 of function match must be string;index:1;value_json:{}",
                    value_json(&other)
                ),
            ))
        }
    };

    let limit_value = match limit {
        JsonValue::Undefined => None,
        JsonValue::Number(num) => Some(num),
        other => {
            return Err(JsonError::new(
                "T0410",
                format!(
                    "Argument 3 of function match must be number;index:3;value_json:{}",
                    value_json(&other)
                ),
            ))
        }
    };
    if let Some(value) = limit_value {
        if value < 0.0 {
            return Err(JsonError::new(
                "D3040",
                format!(
                    "Third argument of match function must evaluate to a positive number;index:3;value_json:{}",
                    value
                ),
            ));
        }
    }

    let mut results = Vec::new();
    let max_matches =
        limit_value.and_then(|value| if value.is_finite() { Some(value.max(0.0) as usize) } else { None });
    if max_matches == Some(0) {
        return Ok(JsonValue::Undefined);
    }

    match matcher {
        JsonValue::Object(_) => {
            let (source, flags) = regex_pattern_from_matcher(&matcher).ok_or_else(|| {
                JsonError::new("T0410", "Argument 2 of function match must be function")
            })?;
            let regex = build_rust_regex(&source, &flags)?;
            for captures in regex.captures_iter(&input_text) {
                if let Some(max) = max_matches {
                    if results.len() >= max {
                        break;
                    }
                }
                let Some(full_match) = captures.get(0) else {
                    continue;
                };
                let groups = captures
                    .iter()
                    .skip(1)
                    .map(|group| match group {
                        Some(value) => JsonValue::String(value.as_str().to_owned()),
                        None => JsonValue::Undefined,
                    })
                    .collect::<Vec<JsonValue>>();
                results.push(matcher_result_to_object(
                    full_match.as_str().to_owned(),
                    full_match.start() as f64,
                    groups,
                ));
            }
        }
        JsonValue::Function(matcher_fn) => {
            let mut current_match =
                evaluate_matcher(focus.clone(), &matcher_fn, Some(input_text.clone())).await?;
            while let Some(matched) = current_match {
                if let Some(max) = max_matches {
                    if results.len() >= max {
                        break;
                    }
                }
                let groups = matched
                    .groups
                    .iter()
                    .map(|group| match group {
                        Some(value) => JsonValue::String(value.clone()),
                        None => JsonValue::Undefined,
                    })
                    .collect::<Vec<JsonValue>>();
                results.push(matcher_result_to_object(
                    matched.match_text.clone(),
                    matched.start as f64,
                    groups,
                ));
                current_match = if let Some(next_matcher) = &matched.next {
                    evaluate_matcher(focus.clone(), next_matcher, None).await?
                } else {
                    None
                };
            }
        }
        _ => {
            return Err(JsonError::new(
                "T0410",
                format!(
                    "Argument 2 of function match must be function;index:2;value_json:{}",
                    value_json(&matcher)
                ),
            ))
        }
    }

    if results.is_empty() {
        return Ok(JsonValue::Undefined);
    }
    if results.len() == 1 {
        return Ok(results.remove(0));
    }
    Ok(JsonValue::Array(JsonArray::new(results, true, false)))
}

pub async fn contains_function(
    focus: FunctionContext,
    input: JsonValue,
    token: JsonValue,
) -> std::result::Result<JsonValue, JsonError> {
    if matches!(input, JsonValue::Undefined) {
        return Ok(JsonValue::Undefined);
    }
    let input_text = match input {
        JsonValue::String(text) => text,
        _ => return Err(JsonError::new("T0410", "Argument 1 of function contains must be string")),
    };

    match token {
        JsonValue::String(pattern) => Ok(JsonValue::Bool(input_text.contains(&pattern))),
        JsonValue::Object(_) => {
            let (source, flags) = regex_pattern_from_matcher(&token).ok_or_else(|| {
                JsonError::new("T0410", "Argument 2 of function contains must be string or function")
            })?;
            let regex = build_rust_regex(&source, &flags)?;
            Ok(JsonValue::Bool(regex.is_match(&input_text)))
        }
        JsonValue::Function(matcher) => {
            let matched = evaluate_matcher(focus, &matcher, Some(input_text)).await?;
            Ok(JsonValue::Bool(matched.is_some()))
        }
        _ => Err(JsonError::new(
            "T0410",
            "Argument 2 of function contains must be string or function",
        )),
    }
}

pub async fn split_function(
    focus: FunctionContext,
    input: JsonValue,
    separator: JsonValue,
    limit: JsonValue,
) -> std::result::Result<JsonValue, JsonError> {
    if matches!(input, JsonValue::Undefined) {
        return Ok(JsonValue::Undefined);
    }
    let input_text = match input {
        JsonValue::String(text) => text,
        _ => return Err(JsonError::new("T0410", "Argument 1 of function split must be string")),
    };
    let limit_value = match limit {
        JsonValue::Undefined => None,
        JsonValue::Number(num) => Some(num),
        _ => return Err(JsonError::new("T0410", "Argument 3 of function split must be number")),
    };
    if let Some(value) = limit_value {
        if value < 0.0 {
            return Err(JsonError::new(
                "D3020",
                "Third argument of split function must evaluate to a positive number",
            ));
        }
    }
    if limit_value.is_some_and(|value| !(value > 0.0)) {
        return Ok(JsonValue::Array(JsonArray::new(Vec::new(), false, false)));
    }
    let max_items = limit_value.map(|value| {
        if value.is_finite() {
            value.trunc().max(0.0) as usize
        } else {
            usize::MAX
        }
    });

    let result: Vec<JsonValue> = match separator {
        JsonValue::String(separator_text) => {
            if separator_text.is_empty() {
                let iter = input_text.chars().map(|ch| JsonValue::String(ch.to_string()));
                match max_items {
                    Some(max) => iter.take(max).collect(),
                    None => iter.collect(),
                }
            } else {
                let iter = input_text
                    .split(&separator_text)
                    .map(|part| JsonValue::String(part.to_owned()));
                match max_items {
                    Some(max) => iter.take(max).collect(),
                    None => iter.collect(),
                }
            }
        }
        JsonValue::Object(_) => {
            let (source, flags) = regex_pattern_from_matcher(&separator).ok_or_else(|| {
                JsonError::new("T0410", "Argument 2 of function split must be string or function")
            })?;
            let regex = build_rust_regex(&source, &flags)?;
            let mut out: Vec<JsonValue> = Vec::new();
            let mut start = 0usize;
            let mut count = 0usize;
            for matched in regex.find_iter(&input_text) {
                if max_items.is_some_and(|max| count >= max) {
                    break;
                }
                if matched.as_str().is_empty() {
                    return Err(JsonError::new(
                        "D1004",
                        "Regular expression matches zero length string",
                    ));
                }
                out.push(JsonValue::String(input_text[start..matched.start()].to_owned()));
                start = matched.end();
                count += 1;
            }
            if !max_items.is_some_and(|max| count >= max) {
                out.push(JsonValue::String(input_text[start..].to_owned()));
            }
            out
        }
        JsonValue::Function(matcher) => {
            let mut out: Vec<JsonValue> = Vec::new();
            let mut count = 0usize;
            let mut start = 0usize;
            let mut current_match =
                evaluate_matcher(focus.clone(), &matcher, Some(input_text.clone())).await?;
            while let Some(matched) = current_match {
                if max_items.is_some_and(|max| count >= max) {
                    break;
                }
                if matched.match_text.is_empty() {
                    return Err(JsonError::new(
                        "D1004",
                        "Regular expression matches zero length string",
                    ));
                }
                let match_start = matched.start.max(start).min(input_text.len());
                let match_end = matched.end.max(match_start).min(input_text.len());
                out.push(JsonValue::String(input_text[start..match_start].to_owned()));
                start = match_end;
                count += 1;
                current_match = if let Some(next_matcher) = &matched.next {
                    evaluate_matcher(focus.clone(), next_matcher, None).await?
                } else {
                    None
                };
            }
            if !max_items.is_some_and(|max| count >= max) {
                out.push(JsonValue::String(input_text[start..].to_owned()));
            }
            out
        }
        _ => {
            return Err(JsonError::new(
                "T0410",
                "Argument 2 of function split must be string or function",
            ))
        }
    };

    Ok(JsonValue::Array(JsonArray::new(result, false, false)))
}

pub async fn replace_function(
    focus: FunctionContext,
    input: JsonValue,
    pattern: JsonValue,
    replacement: JsonValue,
    limit: JsonValue,
) -> std::result::Result<JsonValue, JsonError> {
    if matches!(input, JsonValue::Undefined) {
        return Ok(JsonValue::Undefined);
    }
    let input_text = match input {
        JsonValue::String(text) => text,
        _ => return Err(JsonError::new("T0410", "Argument 1 of function replace must be string")),
    };

    if matches!(pattern, JsonValue::String(ref value) if value.is_empty()) {
        return Err(JsonError::new(
            "D3010",
            "Second argument of replace function cannot be an empty string",
        ));
    }

    let limit_value = match limit {
        JsonValue::Undefined => None,
        JsonValue::Number(num) => Some(num),
        _ => return Err(JsonError::new("T0410", "Argument 4 of function replace must be number")),
    };
    if let Some(value) = limit_value {
        if value < 0.0 {
            return Err(JsonError::new(
                "D3011",
                "Fourth argument of replace function must evaluate to a positive number",
            ));
        }
    }
    if limit_value.is_some_and(|value| !(value > 0.0)) {
        return Ok(JsonValue::String(input_text));
    }

    let replacement_literal = match &replacement {
        JsonValue::String(text) => text.clone(),
        _ => String::new(),
    };
    let replacement_fn = match replacement {
        JsonValue::Function(function) => Some(function),
        _ => None,
    };

    let output = match pattern {
        JsonValue::String(pattern_text) => {
            let mut result = String::new();
            let mut position = 0usize;
            let mut count = 0usize;
            while let Some(relative_index) = input_text[position..].find(&pattern_text) {
                if let Some(limit_num) = limit_value {
                    if (count as f64) >= limit_num {
                        break;
                    }
                }
                let absolute_index = position + relative_index;
                result.push_str(&input_text[position..absolute_index]);
                result.push_str(&replacement_literal);
                position = absolute_index + pattern_text.len();
                count += 1;
            }
            result.push_str(&input_text[position..]);
            result
        }
        JsonValue::Object(_) => {
            let (source, flags) = regex_pattern_from_matcher(&pattern).ok_or_else(|| {
                JsonError::new("T0410", "Argument 2 of function replace must be string or function")
            })?;
            let regex = build_rust_regex(&source, &flags)?;
            let mut result = String::new();
            let mut position = 0usize;
            let mut count = 0usize;
            for captures in regex.captures_iter(&input_text) {
                if let Some(limit_num) = limit_value {
                    if (count as f64) >= limit_num {
                        break;
                    }
                }
                let Some(full_match) = captures.get(0) else {
                    continue;
                };
                if full_match.as_str().is_empty() {
                    return Err(JsonError::new(
                        "D1004",
                        "Regular expression matches zero length string",
                    ));
                }
                result.push_str(&input_text[position..full_match.start()]);
                let regex_match = MatcherResult {
                    match_text: full_match.as_str().to_owned(),
                    start: full_match.start(),
                    end: full_match.end(),
                    groups: captures
                        .iter()
                        .skip(1)
                        .map(|group| group.map(|value| value.as_str().to_owned()))
                        .collect(),
                    next: None,
                };
                let replacement_value = if let Some(function) = &replacement_fn {
                    let called = function
                        .call_forced(focus.clone(), vec![regex_match.callback_object()])
                        .await?;
                    match called {
                        JsonValue::String(text) => text,
                        _ => {
                            return Err(JsonError::new(
                                "D3012",
                                "Attempted to replace a matched string with a non-string value",
                            ))
                        }
                    }
                } else {
                    replacement_string_from_match(&replacement_literal, &regex_match)
                };
                result.push_str(&replacement_value);
                position = full_match.end();
                count += 1;
            }
            result.push_str(&input_text[position..]);
            result
        }
        JsonValue::Function(matcher) => {
            let mut result = String::new();
            let mut position = 0usize;
            let mut count = 0usize;
            let mut current_match =
                evaluate_matcher(focus.clone(), &matcher, Some(input_text.clone())).await?;
            while let Some(matched) = current_match {
                if let Some(limit_num) = limit_value {
                    if (count as f64) >= limit_num {
                        break;
                    }
                }
                if matched.match_text.is_empty() {
                    return Err(JsonError::new(
                        "D1004",
                        "Regular expression matches zero length string",
                    ));
                }
                let start = matched.start.max(position).min(input_text.len());
                result.push_str(&input_text[position..start]);
                let replacement_value = if let Some(function) = &replacement_fn {
                    let called = function
                        .call_forced(focus.clone(), vec![matched.callback_object()])
                        .await?;
                    match called {
                        JsonValue::String(text) => text,
                        _ => {
                            return Err(JsonError::new(
                                "D3012",
                                "Attempted to replace a matched string with a non-string value",
                            ))
                        }
                    }
                } else {
                    replacement_string_from_match(&replacement_literal, &matched)
                };
                result.push_str(&replacement_value);
                let end = matched.end.max(start).min(input_text.len());
                position = end;
                count += 1;
                current_match = if let Some(next_matcher) = &matched.next {
                    evaluate_matcher(focus.clone(), next_matcher, None).await?
                } else {
                    None
                };
            }
            result.push_str(&input_text[position..]);
            result
        }
        _ => {
            return Err(JsonError::new(
                "T0410",
                "Argument 2 of function replace must be string or function",
            ))
        }
    };

    Ok(JsonValue::String(output))
}

pub fn join_function(values: JsonValue, separator: JsonValue) -> std::result::Result<JsonValue, JsonError> {
    if matches!(values, JsonValue::Undefined) {
        return Ok(JsonValue::Undefined);
    }
    let sep = match separator {
        JsonValue::Undefined => String::new(),
        JsonValue::String(text) => text,
        _ => return Err(JsonError::new("T0410", "Argument 2 of function join must be string")),
    };

    let array = match values {
        JsonValue::String(text) => vec![text],
        JsonValue::Array(array) => {
            let mut output: Vec<String> = Vec::with_capacity(array.elements.len());
            for value in array.elements {
                match value {
                    JsonValue::String(text) => output.push(text),
                    _ => {
                        return Err(JsonError::new(
                            "T0412",
                            "Argument 1 of function join must be an array of strings",
                        ))
                    }
                }
            }
            output
        }
        _ => {
            return Err(JsonError::new(
                "T0412",
                "Argument 1 of function join must be an array of strings",
            ))
        }
    };

    Ok(JsonValue::String(array.join(&sep)))
}
