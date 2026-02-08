use serde_json::Value;

pub(crate) struct SignatureValidationError {
    pub(crate) code: &'static str,
    pub(crate) offset: usize,
    pub(crate) value: Option<Value>,
}

pub(crate) fn validate_signature_definition(signature: &str) -> Result<(), SignatureValidationError> {
    let chars: Vec<char> = signature.chars().collect();
    let mut position = 1usize;
    let mut previous_param_type: Option<char> = None;

    while position < chars.len() {
        let symbol = chars[position];
        if symbol == ':' {
            break;
        }

        match symbol {
            's' | 'n' | 'b' | 'l' | 'o' | 'a' | 'f' | 'j' | 'x' => {
                previous_param_type = Some(symbol);
            }
            '-' | '?' | '+' => {}
            '(' => {
                let end = find_closing_bracket(&chars, position, '(', ')');
                if end <= chars.len() {
                    let has_parameterized_type = chars[position + 1..end].contains(&'<');
                    if has_parameterized_type {
                        return Err(SignatureValidationError {
                            code: "S0402",
                            offset: position,
                            value: Some(Value::String(
                                chars[position + 1..end].iter().collect::<String>(),
                            )),
                        });
                    }
                    previous_param_type = Some('(');
                }
                position = end;
            }
            '<' => {
                let prev_type = previous_param_type.unwrap_or('\0');
                if prev_type != 'a' && prev_type != 'f' {
                    let value = if prev_type == '\0' {
                        Value::Null
                    } else {
                        Value::String(prev_type.to_string())
                    };
                    return Err(SignatureValidationError {
                        code: "S0401",
                        offset: position,
                        value: Some(value),
                    });
                }
                position = find_closing_bracket(&chars, position, '<', '>');
            }
            _ => {}
        }

        position += 1;
    }

    Ok(())
}

fn find_closing_bracket(chars: &[char], start: usize, open_symbol: char, close_symbol: char) -> usize {
    let mut depth = 1usize;
    let mut position = start;

    while position + 1 < chars.len() {
        position += 1;
        match chars[position] {
            symbol if symbol == close_symbol => {
                depth -= 1;
                if depth == 0 {
                    return position;
                }
            }
            symbol if symbol == open_symbol => {
                depth += 1;
            }
            _ => {}
        }
    }

    chars.len().saturating_sub(1)
}
