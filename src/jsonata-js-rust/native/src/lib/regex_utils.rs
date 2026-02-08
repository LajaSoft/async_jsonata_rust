use super::*;

fn get_object_property<'a>(object: &'a JsonObject, key: &str) -> Option<&'a JsonValue> {
    object.0.iter().find_map(|(name, value)| {
        if name == key {
            return Some(value);
        }
        None
    })
}

pub(crate) fn matcher_result_to_object(
    match_text: String,
    start: f64,
    groups: Vec<JsonValue>,
) -> JsonValue {
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
pub(crate) struct MatcherResult {
    pub(crate) match_text: String,
    pub(crate) start: usize,
    pub(crate) groups: Vec<Option<String>>,
    pub(crate) next: Option<JsonFunction>,
}

impl MatcherResult {
    pub(crate) fn callback_object(&self) -> JsonValue {
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

fn matcher_result_from_json(
    value: JsonValue,
) -> std::result::Result<Option<MatcherResult>, JsonError> {
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

pub(crate) async fn evaluate_matcher(
    focus: FunctionContext,
    matcher: &JsonFunction,
    input: Option<String>,
) -> std::result::Result<Option<MatcherResult>, JsonError> {
    let mut args = Vec::with_capacity(1);
    match input {
        Some(text) => args.push(JsonValue::String(text)),
        None => args.push(JsonValue::Undefined),
    }
    let result = matcher.call(focus, args).await?;
    matcher_result_from_json(result)
}

pub(crate) fn replacement_string_from_match(
    replacement: &str,
    regex_match: &MatcherResult,
) -> String {
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
        let mut capture_index = first_slice.parse::<usize>().ok();
        if max_digits > 1 {
            if let Some(parsed) = capture_index {
                if parsed > regex_match.groups.len() && first_len > 1 {
                    let second_slice = &replacement[position..position + first_len - 1];
                    capture_index = second_slice.parse::<usize>().ok();
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

pub(crate) fn regex_pattern_from_matcher(value: &JsonValue) -> Option<(String, String)> {
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

pub(crate) fn build_rust_regex(
    source: &str,
    flags: &str,
) -> std::result::Result<regex::Regex, JsonError> {
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
        .map_err(|err| JsonError::new("T1010", format!("Invalid regex: {}", err)))
}
