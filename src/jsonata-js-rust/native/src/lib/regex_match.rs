use super::*;

pub(crate) async fn replace_function_impl(
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
        other => {
            return Err(JsonError::new(
                "T0410",
                format!("Argument 1 of function replace must be string, got {:?}", other),
            ))
        }
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

    match pattern {
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
            Ok(JsonValue::String(result))
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
                result.push_str(&input_text[position..full_match.start()]);
                let regex_match = MatcherResult {
                    match_text: full_match.as_str().to_owned(),
                    start: full_match.start(),
                    groups: captures
                        .iter()
                        .skip(1)
                        .map(|group| group.map(|value| value.as_str().to_owned()))
                        .collect(),
                    next: None,
                };
                let replacement_value = if let Some(function) = &replacement_fn {
                    let called = function
                        .call(focus.clone(), vec![regex_match.callback_object()])
                        .await?;
                    match called {
                        JsonValue::String(text) => text,
                        other => {
                            return Err(JsonError::new(
                                "D3012",
                                format!(
                                    "Attempted to replace a matched string with a non-string value: {:?}",
                                    other
                                ),
                            ))
                        }
                    }
                } else {
                    replacement_string_from_match(&replacement_literal, &regex_match)
                };
                result.push_str(&replacement_value);
                position = full_match.start() + full_match.as_str().len();
                count += 1;
            }

            result.push_str(&input_text[position..]);
            Ok(JsonValue::String(result))
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
                let start = matched.start.max(position).min(input_text.len());
                result.push_str(&input_text[position..start]);
                let replacement_value = if let Some(function) = &replacement_fn {
                    let called = function
                        .call(focus.clone(), vec![matched.callback_object()])
                        .await?;
                    match called {
                        JsonValue::String(text) => text,
                        other => {
                            return Err(JsonError::new(
                                "D3012",
                                format!(
                                    "Attempted to replace a matched string with a non-string value: {:?}",
                                    other
                                ),
                            ))
                        }
                    }
                } else {
                    replacement_string_from_match(&replacement_literal, &matched)
                };
                result.push_str(&replacement_value);
                let end = start.saturating_add(matched.match_text.len()).min(input_text.len());
                position = end;
                count += 1;

                current_match = if let Some(next_matcher) = &matched.next {
                    evaluate_matcher(focus.clone(), next_matcher, None).await?
                } else {
                    None
                };
            }

            result.push_str(&input_text[position..]);
            Ok(JsonValue::String(result))
        }
        _ => Err(JsonError::new(
            "T0410",
            "Argument 2 of function replace must be string or function",
        )),
    }
}

pub(crate) async fn match_function_impl(
    _focus: FunctionContext,
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
                format!("Argument 1 of function match must be string, got {:?}", other),
            ))
        }
    };

    let (source, flags) = match regex_pattern_from_matcher(&matcher) {
        Some(value) => value,
        None => {
            return Err(JsonError::new(
                "T0410",
                "Argument 2 of function match must be function",
            ))
        }
    };

    let limit_value = match limit {
        JsonValue::Undefined => None,
        JsonValue::Number(num) => Some(num),
        _ => return Err(JsonError::new("T0410", "Argument 3 of function match must be number")),
    };

    if let Some(value) = limit_value {
        if value < 0.0 {
            return Err(JsonError::new(
                "D3040",
                "Third argument of match function must evaluate to a positive number",
            ));
        }
    }

    let regex = build_rust_regex(&source, &flags)?;
    let mut results: Vec<JsonValue> = Vec::new();
    let max_matches =
        limit_value.and_then(|value| if value.is_finite() { Some(value.max(0.0) as usize) } else { None });

    if max_matches == Some(0) {
        return Ok(JsonValue::Array(JsonArray::new(results, true, false)));
    }

    for captures in regex.captures_iter(&input_text) {
        if let Some(max) = max_matches {
            if results.len() >= max {
                break;
            }
        }
        let full_match = match captures.get(0) {
            Some(value) => value,
            None => continue,
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

    Ok(JsonValue::Array(JsonArray::new(results, true, false)))
}
