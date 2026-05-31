//! Runtime validation of lambda type signatures.
//!
//! Ports the `parseSignature(...).validate(args, context)` logic from the
//! reference JSONata engine. A signature like `<s-s:s>` is compiled into a set
//! of parameters; given a list of supplied arguments (and the current context
//! value) it returns the validated/fixed-up argument list, or an error.

use regex::Regex;

use crate::types::{JsonError, JsonValue};

#[derive(Clone)]
struct Param {
    regex: String,
    /// Base type symbol for this parameter (`s`, `n`, `a`, `f`, ...). `(` for a
    /// choice group.
    type_symbol: char,
    /// Optional contained-type for array/function parameters (e.g. `n` in `a<n>`).
    subtype: Option<String>,
    /// When `true`, the context value is substituted if the argument is missing.
    context: bool,
    /// Pre-compiled regex used to test the context value's type symbol.
    context_regex: Option<String>,
}

#[derive(Clone)]
pub(super) struct Signature {
    params: Vec<Param>,
    regex: Regex,
}

/// Returns the single-character type symbol used by the reference engine.
fn get_symbol(value: &JsonValue) -> char {
    match value {
        JsonValue::Function(_) => 'f',
        JsonValue::String(_) => 's',
        JsonValue::Number(_) => 'n',
        JsonValue::Bool(_) => 'b',
        JsonValue::Null => 'l',
        JsonValue::Array(_) => 'a',
        JsonValue::Object(_) => 'o',
        JsonValue::Undefined => 'm',
    }
}

fn find_closing_bracket(chars: &[char], start: usize, open: char, close: char) -> usize {
    let mut depth = 1usize;
    let mut position = start;
    while position + 1 < chars.len() {
        position += 1;
        let symbol = chars[position];
        if symbol == close {
            depth -= 1;
            if depth == 0 {
                return position;
            }
        } else if symbol == open {
            depth += 1;
        }
    }
    position
}

impl Signature {
    /// Compiles a signature string such as `<s-s:s>`. Returns `None` when the
    /// string is not a usable signature.
    pub(super) fn parse(signature: &str) -> Option<Signature> {
        let chars: Vec<char> = signature.chars().collect();
        if chars.is_empty() {
            return None;
        }
        let mut params: Vec<Param> = Vec::new();
        let mut position = 1usize; // skip leading '<'

        let mut pending: Option<Param> = None;

        // Helper to flush the pending param.
        macro_rules! push_pending {
            () => {
                if let Some(p) = pending.take() {
                    params.push(p);
                }
            };
        }

        while position < chars.len() {
            let symbol = chars[position];
            if symbol == ':' {
                break;
            }
            match symbol {
                's' | 'n' | 'b' | 'l' | 'o' => {
                    push_pending!();
                    pending = Some(Param {
                        regex: format!("[{symbol}m]"),
                        type_symbol: symbol,
                        subtype: None,
                        context: false,
                        context_regex: None,
                    });
                }
                'a' => {
                    push_pending!();
                    pending = Some(Param {
                        regex: "[asnblfom]".to_owned(),
                        type_symbol: 'a',
                        subtype: None,
                        context: false,
                        context_regex: None,
                    });
                }
                'f' => {
                    push_pending!();
                    pending = Some(Param {
                        regex: "f".to_owned(),
                        type_symbol: 'f',
                        subtype: None,
                        context: false,
                        context_regex: None,
                    });
                }
                'j' => {
                    push_pending!();
                    pending = Some(Param {
                        regex: "[asnblom]".to_owned(),
                        type_symbol: 'j',
                        subtype: None,
                        context: false,
                        context_regex: None,
                    });
                }
                'x' => {
                    push_pending!();
                    pending = Some(Param {
                        regex: "[asnblfom]".to_owned(),
                        type_symbol: 'x',
                        subtype: None,
                        context: false,
                        context_regex: None,
                    });
                }
                '(' => {
                    push_pending!();
                    let end = find_closing_bracket(&chars, position, '(', ')');
                    let choice: String = chars[position + 1..end].iter().collect();
                    if choice.contains('<') {
                        // Parameterized choice types are unsupported (S0402 at
                        // parse time); bail out of validation.
                        return None;
                    }
                    pending = Some(Param {
                        regex: format!("[{choice}m]"),
                        type_symbol: '(',
                        subtype: None,
                        context: false,
                        context_regex: None,
                    });
                    position = end;
                }
                '-' => {
                    if let Some(p) = pending.as_mut() {
                        p.context = true;
                        p.context_regex = Some(format!("^{}$", p.regex));
                        p.regex.push('?');
                    }
                }
                '?' | '+' => {
                    if let Some(p) = pending.as_mut() {
                        p.regex.push(symbol);
                    }
                }
                '<' => {
                    let end = find_closing_bracket(&chars, position, '<', '>');
                    let subtype: String = chars[position + 1..end].iter().collect();
                    if let Some(p) = pending.as_mut() {
                        if p.type_symbol == 'a' || p.type_symbol == 'f' {
                            p.subtype = Some(subtype);
                        }
                    }
                    position = end;
                }
                _ => {}
            }
            position += 1;
        }
        push_pending!();

        let regex_str = format!(
            "^{}$",
            params
                .iter()
                .map(|p| format!("({})", p.regex))
                .collect::<String>()
        );
        let regex = Regex::new(&regex_str).ok()?;
        Some(Signature { params, regex })
    }

    /// Validates and fixes up the supplied arguments. Returns the validated
    /// arguments, or a `JsonError` (T0410/T0411/T0412).
    pub(super) fn validate(
        &self,
        args: Vec<JsonValue>,
        context: &JsonValue,
    ) -> Result<Vec<JsonValue>, JsonError> {
        let supplied_sig: String = args.iter().map(get_symbol).collect();
        let captures = match self.regex.captures(&supplied_sig) {
            Some(c) => c,
            None => return Err(self.validation_error(&args, &supplied_sig)),
        };

        let mut validated: Vec<JsonValue> = Vec::new();
        let mut arg_index = 0usize;
        for (index, param) in self.params.iter().enumerate() {
            let matched = captures
                .get(index + 1)
                .map(|m| m.as_str())
                .unwrap_or("");
            if matched.is_empty() {
                if param.context {
                    let context_type = get_symbol(context).to_string();
                    let context_ok = param
                        .context_regex
                        .as_ref()
                        .and_then(|r| Regex::new(r).ok())
                        .map(|r| r.is_match(&context_type))
                        .unwrap_or(false);
                    if context_ok {
                        validated.push(context.clone());
                    } else {
                        return Err(JsonError::new(
                            "T0411",
                            format!(
                                "Context value is not a compatible type with argument {} of function",
                                arg_index + 1
                            ),
                        ));
                    }
                } else {
                    validated.push(args.get(arg_index).cloned().unwrap_or(JsonValue::Undefined));
                    arg_index += 1;
                }
            } else {
                for single in matched.chars() {
                    if param.type_symbol == 'a' {
                        if single == 'm' {
                            validated.push(JsonValue::Undefined);
                        } else {
                            let mut arg =
                                args.get(arg_index).cloned().unwrap_or(JsonValue::Undefined);
                            let mut array_ok = true;
                            if let Some(subtype) = &param.subtype {
                                let subtype_first = subtype.chars().next().unwrap_or('\0');
                                if single != 'a' && Some(matched) != Some(subtype.as_str()) {
                                    array_ok = false;
                                } else if single == 'a' {
                                    if let JsonValue::Array(array) = &arg {
                                        if !array.elements.is_empty() {
                                            let item_type = get_symbol(&array.elements[0]);
                                            if item_type != subtype_first {
                                                array_ok = false;
                                            } else {
                                                array_ok = array
                                                    .elements
                                                    .iter()
                                                    .all(|v| get_symbol(v) == item_type);
                                            }
                                        }
                                    }
                                }
                            }
                            if !array_ok {
                                return Err(JsonError::new(
                                    "T0412",
                                    format!(
                                        "Argument {} of function must be an array of {}",
                                        arg_index + 1,
                                        param.subtype.clone().unwrap_or_default()
                                    ),
                                ));
                            }
                            // The function expects an array; wrap a singleton.
                            if single != 'a' {
                                arg = JsonValue::Array(crate::types::JsonArray::new(
                                    vec![arg],
                                    false,
                                    false,
                                ));
                            }
                            validated.push(arg);
                        }
                        arg_index += 1;
                    } else {
                        validated
                            .push(args.get(arg_index).cloned().unwrap_or(JsonValue::Undefined));
                        arg_index += 1;
                    }
                }
            }
        }
        Ok(validated)
    }

    fn validation_error(&self, args: &[JsonValue], supplied_sig: &str) -> JsonError {
        // Re-apply each parameter regex incrementally to find the first failure.
        let mut partial = String::from("^");
        let mut good_to = 0usize;
        for param in &self.params {
            partial.push_str(&param.regex);
            match Regex::new(&partial) {
                Ok(re) => match re.find(supplied_sig) {
                    Some(m) => good_to = m.as_str().len(),
                    None => {
                        return JsonError::new(
                            "T0410",
                            format!(
                                "Argument {} of function does not match function signature",
                                good_to + 1
                            ),
                        );
                    }
                },
                Err(_) => break,
            }
        }
        let _ = args;
        JsonError::new(
            "T0410",
            format!(
                "Argument {} of function does not match function signature",
                good_to + 1
            ),
        )
    }
}
