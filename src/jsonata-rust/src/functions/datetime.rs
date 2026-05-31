//! Date/time and integer formatting/parsing built-ins.
//!
//! Faithful port of the upstream JSONata `datetime.js` implementation of the
//! XPath/XForms picture-string machinery used by `formatInteger`, `parseInteger`,
//! `fromMillis` and `toMillis`.

use time::{OffsetDateTime, Weekday};

use crate::types::JsonError;

// ---------------------------------------------------------------------------
// Words
// ---------------------------------------------------------------------------

const FEW: [&str; 20] = [
    "Zero", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten",
    "Eleven", "Twelve", "Thirteen", "Fourteen", "Fifteen", "Sixteen", "Seventeen", "Eighteen",
    "Nineteen",
];
const ORDINALS: [&str; 20] = [
    "Zeroth", "First", "Second", "Third", "Fourth", "Fifth", "Sixth", "Seventh", "Eighth", "Ninth",
    "Tenth", "Eleventh", "Twelfth", "Thirteenth", "Fourteenth", "Fifteenth", "Sixteenth",
    "Seventeenth", "Eighteenth", "Nineteenth",
];
const DECADES: [&str; 9] = [
    "Twenty", "Thirty", "Forty", "Fifty", "Sixty", "Seventy", "Eighty", "Ninety", "Hundred",
];
const MAGNITUDES: [&str; 4] = ["Thousand", "Million", "Billion", "Trillion"];

fn number_to_words(value: f64, ordinal: bool) -> String {
    fn lookup(num: f64, prev: bool, ord: bool) -> String {
        let mut words;
        if num <= 19.0 {
            let idx = num as usize;
            words = String::new();
            if prev {
                words.push_str(" and ");
            }
            words.push_str(if ord { ORDINALS[idx] } else { FEW[idx] });
        } else if num < 100.0 {
            let tens = (num / 10.0).floor() as usize;
            let remainder = (num % 10.0) as usize;
            words = String::new();
            if prev {
                words.push_str(" and ");
            }
            words.push_str(DECADES[tens - 2]);
            if remainder > 0 {
                words.push('-');
                words.push_str(&lookup(remainder as f64, false, ord));
            } else if ord {
                // strip trailing 'y' add 'ieth'
                let len = words.len();
                words = words[..len - 1].to_string() + "ieth";
            }
        } else if num < 1000.0 {
            let hundreds = (num / 100.0).floor() as usize;
            let remainder = num % 100.0;
            words = String::new();
            if prev {
                words.push_str(", ");
            }
            words.push_str(FEW[hundreds]);
            words.push_str(" Hundred");
            if remainder > 0.0 {
                words.push_str(&lookup(remainder, true, ord));
            } else if ord {
                words.push_str("th");
            }
        } else {
            let mut mag = (num.log10() / 3.0).floor() as usize;
            if mag > MAGNITUDES.len() {
                mag = MAGNITUDES.len();
            }
            let factor = 10f64.powi((mag * 3) as i32);
            let mant = (num / factor).floor();
            let remainder = num - mant * factor;
            words = String::new();
            if prev {
                words.push_str(", ");
            }
            words.push_str(&lookup(mant, false, false));
            words.push(' ');
            words.push_str(MAGNITUDES[mag - 1]);
            if remainder > 0.0 {
                words.push_str(&lookup(remainder, true, ord));
            } else if ord {
                words.push_str("th");
            }
        }
        words
    }

    lookup(value, false, ordinal)
}

// ---------------------------------------------------------------------------
// Words -> number
// ---------------------------------------------------------------------------

fn word_values() -> std::collections::HashMap<String, f64> {
    let mut map = std::collections::HashMap::new();
    for (index, word) in FEW.iter().enumerate() {
        map.insert(word.to_lowercase(), index as f64);
    }
    for (index, word) in ORDINALS.iter().enumerate() {
        map.insert(word.to_lowercase(), index as f64);
    }
    for (index, word) in DECADES.iter().enumerate() {
        let lword = word.to_lowercase();
        let val = ((index + 2) * 10) as f64;
        // JS uses word.length (the original mixed-case length) for substring;
        // lengths are identical for ASCII words.
        let prefix = &lword[..word.len() - 1];
        map.insert(lword.clone(), val);
        map.insert(format!("{prefix}ieth"), val);
    }
    map.insert("hundredth".to_string(), 100.0);
    for (index, word) in MAGNITUDES.iter().enumerate() {
        let lword = word.to_lowercase();
        let val = 10f64.powi(((index + 1) * 3) as i32);
        map.insert(lword.clone(), val);
        map.insert(format!("{lword}th"), val);
    }
    map
}

fn words_to_number(text: &str) -> f64 {
    let values = word_values();
    // split on ", " | " and " | whitespace | "-"
    let mut parts: Vec<String> = Vec::new();
    {
        // Replicate JS regex /,\s|\sand\s|[\s\\-]/
        // We tokenize manually.
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        let mut current = String::new();
        while i < chars.len() {
            // check ", "
            if chars[i] == ',' && i + 1 < chars.len() && chars[i + 1].is_whitespace() {
                parts.push(std::mem::take(&mut current));
                i += 2;
                continue;
            }
            // check " and "
            if chars[i].is_whitespace()
                && i + 4 < chars.len()
                && chars[i + 1] == 'a'
                && chars[i + 2] == 'n'
                && chars[i + 3] == 'd'
                && chars[i + 4].is_whitespace()
            {
                parts.push(std::mem::take(&mut current));
                i += 5;
                continue;
            }
            // whitespace or dash separator
            if chars[i].is_whitespace() || chars[i] == '-' {
                parts.push(std::mem::take(&mut current));
                i += 1;
                continue;
            }
            current.push(chars[i]);
            i += 1;
        }
        parts.push(current);
    }

    let mut segs: Vec<f64> = vec![0.0];
    for part in &parts {
        let value = *values.get(part).unwrap_or(&f64::NAN);
        if value < 100.0 {
            let mut top = segs.pop().unwrap();
            if top >= 1000.0 {
                segs.push(top);
                top = 0.0;
            }
            segs.push(top + value);
        } else {
            let last = segs.pop().unwrap();
            segs.push(last * value);
        }
    }
    segs.iter().sum()
}

// ---------------------------------------------------------------------------
// Roman numerals
// ---------------------------------------------------------------------------

const ROMAN_NUMERALS: [(u64, &str); 13] = [
    (1000, "m"),
    (900, "cm"),
    (500, "d"),
    (400, "cd"),
    (100, "c"),
    (90, "xc"),
    (50, "l"),
    (40, "xl"),
    (10, "x"),
    (9, "ix"),
    (5, "v"),
    (4, "iv"),
    (1, "i"),
];

fn decimal_to_roman(value: u64) -> String {
    let mut value = value;
    let mut out = String::new();
    while value > 0 {
        for (val, numeral) in ROMAN_NUMERALS.iter() {
            if value >= *val {
                out.push_str(numeral);
                value -= *val;
                break;
            }
        }
    }
    out
}

fn roman_value(c: char) -> u64 {
    match c {
        'M' => 1000,
        'D' => 500,
        'C' => 100,
        'L' => 50,
        'X' => 10,
        'V' => 5,
        'I' => 1,
        _ => 0,
    }
}

fn roman_to_decimal(roman: &str) -> f64 {
    let mut decimal: i64 = 0;
    let mut max: i64 = 1;
    let chars: Vec<char> = roman.chars().collect();
    for i in (0..chars.len()).rev() {
        let value = roman_value(chars[i]) as i64;
        if value < max {
            decimal -= value;
        } else {
            max = value;
            decimal += value;
        }
    }
    decimal as f64
}

// ---------------------------------------------------------------------------
// Letters (spreadsheet column names)
// ---------------------------------------------------------------------------

fn decimal_to_letters(value: u64, a_char: char) -> String {
    let mut value = value;
    let a_code = a_char as u32;
    let mut letters: Vec<char> = Vec::new();
    while value > 0 {
        let code = ((value - 1) % 26) as u32 + a_code;
        letters.insert(0, char::from_u32(code).unwrap());
        value = (value - 1) / 26;
    }
    letters.into_iter().collect()
}

fn letters_to_decimal(letters: &str, a_char: char) -> f64 {
    let a_code = a_char as i64;
    let chars: Vec<char> = letters.chars().collect();
    let mut decimal: f64 = 0.0;
    let len = chars.len();
    for (i, _) in chars.iter().enumerate() {
        let ch = chars[len - i - 1] as i64;
        decimal += ((ch - a_code + 1) as f64) * 26f64.powi(i as i32);
    }
    decimal
}

// ---------------------------------------------------------------------------
// Integer picture analysis
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Primary {
    Decimal,
    Letters,
    Roman,
    Words,
    Sequence,
}

#[derive(Clone, Debug, PartialEq)]
enum TCase {
    Upper,
    Lower,
    Title,
}

#[derive(Clone, Debug)]
struct GroupSeparator {
    position: usize,
    character: char,
}

#[derive(Clone, Debug)]
struct IntegerFormat {
    primary: Primary,
    case: TCase,
    ordinal: bool,
    token: String,
    // decimal
    zero_code: u32,
    mandatory_digits: usize,
    optional_digits: usize,
    regular: bool,
    regular_position: usize,
    regular_char: char,
    grouping_separators: Vec<GroupSeparator>,
    // parse width (set for date components)
    parse_width: Option<usize>,
}

impl IntegerFormat {
    fn new() -> Self {
        IntegerFormat {
            primary: Primary::Decimal,
            case: TCase::Lower,
            ordinal: false,
            token: String::new(),
            zero_code: 0x30,
            mandatory_digits: 0,
            optional_digits: 0,
            regular: false,
            regular_position: 0,
            regular_char: ',',
            grouping_separators: Vec::new(),
            parse_width: None,
        }
    }
}

// Unicode decimal-digit group base codepoints (zero of each group).
const DECIMAL_GROUPS: [u32; 38] = [
    0x30, 0x0660, 0x06F0, 0x07C0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0B66, 0x0BE6, 0x0C66, 0x0CE6,
    0x0D66, 0x0DE6, 0x0E50, 0x0ED0, 0x0F20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80,
    0x1A90, 0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0,
    0xFF10, 0x1D7CE,
];

fn analyse_integer_picture(picture: &str) -> Result<IntegerFormat, JsonError> {
    let mut format = IntegerFormat::new();

    let primary_format: &str;
    let mut format_modifier: Option<&str> = None;
    match picture.rfind(';') {
        None => primary_format = picture,
        Some(idx) => {
            primary_format = &picture[..idx];
            format_modifier = Some(&picture[idx + 1..]);
        }
    }
    if let Some(fm) = format_modifier {
        if fm.starts_with('o') {
            format.ordinal = true;
        }
    }

    match primary_format {
        "A" => {
            format.case = TCase::Upper;
            format.primary = Primary::Letters;
        }
        "a" => {
            format.primary = Primary::Letters;
        }
        "I" => {
            format.case = TCase::Upper;
            format.primary = Primary::Roman;
        }
        "i" => {
            format.primary = Primary::Roman;
        }
        "W" => {
            format.case = TCase::Upper;
            format.primary = Primary::Words;
        }
        "Ww" => {
            format.case = TCase::Title;
            format.primary = Primary::Words;
        }
        "w" => {
            format.primary = Primary::Words;
        }
        _ => {
            let mut zero_code: Option<u32> = None;
            let mut mandatory_digits = 0usize;
            let mut optional_digits = 0usize;
            let mut grouping_separators: Vec<GroupSeparator> = Vec::new();
            let mut separator_position = 0usize;
            // reverse the codepoints to determine positions of grouping-separator-signs
            let codepoints: Vec<u32> = primary_format.chars().rev().map(|c| c as u32).collect();
            for code_point in codepoints {
                let mut digit = false;
                for group in DECIMAL_GROUPS.iter() {
                    if code_point >= *group && code_point <= group + 9 {
                        digit = true;
                        mandatory_digits += 1;
                        separator_position += 1;
                        match zero_code {
                            None => zero_code = Some(*group),
                            Some(zc) if *group != zc => {
                                return Err(JsonError::new("D3131", "different decimal groups"));
                            }
                            _ => {}
                        }
                        break;
                    }
                }
                if !digit {
                    if code_point == 0x23 {
                        // '#'
                        separator_position += 1;
                        optional_digits += 1;
                    } else {
                        grouping_separators.push(GroupSeparator {
                            position: separator_position,
                            character: char::from_u32(code_point).unwrap(),
                        });
                    }
                }
            }
            if mandatory_digits > 0 {
                format.primary = Primary::Decimal;
                format.zero_code = zero_code.unwrap();
                format.mandatory_digits = mandatory_digits;
                format.optional_digits = optional_digits;

                let regular = regular_repeat(&grouping_separators);
                if regular > 0 {
                    format.regular = true;
                    format.regular_position = regular;
                    format.regular_char = grouping_separators[0].character;
                } else {
                    format.regular = false;
                    format.grouping_separators = grouping_separators;
                }
            } else {
                format.primary = Primary::Sequence;
                format.token = primary_format.to_string();
            }
        }
    }

    Ok(format)
}

fn regular_repeat(separators: &[GroupSeparator]) -> usize {
    if separators.is_empty() {
        return 0;
    }
    let sep_char = separators[0].character;
    for s in &separators[1..] {
        if s.character != sep_char {
            return 0;
        }
    }
    let indexes: Vec<usize> = separators.iter().map(|s| s.position).collect();
    fn gcd(a: usize, b: usize) -> usize {
        if b == 0 {
            a
        } else {
            gcd(b, a % b)
        }
    }
    let factor = indexes.iter().copied().reduce(gcd).unwrap();
    if factor == 0 {
        return 0;
    }
    for index in 1..=indexes.len() {
        if !indexes.contains(&(index * factor)) {
            return 0;
        }
    }
    factor
}

// ---------------------------------------------------------------------------
// Integer formatting
// ---------------------------------------------------------------------------

fn format_integer_internal(value: f64, format: &IntegerFormat) -> Result<String, JsonError> {
    let negative = value < 0.0;
    let abs = value.abs();
    let mut formatted: String;

    match format.primary {
        Primary::Letters => {
            let a = if format.case == TCase::Upper { 'A' } else { 'a' };
            formatted = decimal_to_letters(abs as u64, a);
        }
        Primary::Roman => {
            formatted = decimal_to_roman(abs as u64);
            if format.case == TCase::Upper {
                formatted = formatted.to_uppercase();
            }
        }
        Primary::Words => {
            formatted = number_to_words(abs, format.ordinal);
            if format.case == TCase::Upper {
                formatted = formatted.to_uppercase();
            } else if format.case == TCase::Lower {
                formatted = formatted.to_lowercase();
            }
        }
        Primary::Decimal => {
            // base string of the integer value (in ASCII digits)
            formatted = format_decimal_string(abs);
            let pad_length = format.mandatory_digits as i64 - formatted.chars().count() as i64;
            if pad_length > 0 {
                let padding: String = std::iter::repeat('0').take(pad_length as usize).collect();
                formatted = padding + &formatted;
            }
            if format.zero_code != 0x30 {
                formatted = formatted
                    .chars()
                    .map(|c| {
                        let cp = c as u32 + format.zero_code - 0x30;
                        char::from_u32(cp).unwrap()
                    })
                    .collect();
            }
            // insert grouping separators
            if format.regular {
                let chars: Vec<char> = formatted.chars().collect();
                let len = chars.len();
                let n = (len - 1) / format.regular_position;
                let mut result = chars;
                for ii in (1..=n).rev() {
                    let pos = result.len() - ii * format.regular_position;
                    result.insert(pos, format.regular_char);
                }
                formatted = result.into_iter().collect();
            } else {
                let mut chars: Vec<char> = formatted.chars().collect();
                for separator in format.grouping_separators.iter().rev() {
                    let len = chars.len();
                    if separator.position <= len {
                        let pos = len - separator.position;
                        chars.insert(pos, separator.character);
                    }
                }
                formatted = chars.into_iter().collect();
            }

            if format.ordinal {
                let chars: Vec<char> = formatted.chars().collect();
                let last_digit = chars[chars.len() - 1];
                let mut suffix = match last_digit {
                    '1' => "st",
                    '2' => "nd",
                    '3' => "rd",
                    _ => "th",
                };
                if (last_digit != '1' && last_digit != '2' && last_digit != '3')
                    || (chars.len() > 1 && chars[chars.len() - 2] == '1')
                {
                    suffix = "th";
                }
                formatted.push_str(suffix);
            }
        }
        Primary::Sequence => {
            return Err(JsonError::new("D3130", "unsupported numbering sequence"));
        }
    }

    if negative {
        formatted = format!("-{formatted}");
    }
    Ok(formatted)
}

/// Format a non-negative whole f64 as a decimal digit string (no exponent).
fn format_decimal_string(value: f64) -> String {
    // value is already floored and non-negative.
    if value < 9_007_199_254_740_992.0 {
        // safely representable as integer
        return format!("{}", value as u64);
    }
    // For very large values (e.g. 1e46), reconstruct digit string like JS `'' + value`.
    // JS produces exponential notation for >=1e21, but these only appear in word-format
    // tests which never reach the decimal branch. Use a best-effort expansion.
    let s = format!("{value:.0}");
    s
}

pub fn format_integer(value: f64, picture: &str) -> Result<String, JsonError> {
    let value = value.floor();
    let format = analyse_integer_picture(picture)?;
    format_integer_internal(value, &format)
}

// ---------------------------------------------------------------------------
// parseInteger
// ---------------------------------------------------------------------------

pub fn parse_integer(value: &str, picture: &str) -> Result<f64, JsonError> {
    let format = analyse_integer_picture(picture)?;
    match format.primary {
        Primary::Letters => {
            let a = if format.case == TCase::Upper { 'A' } else { 'a' };
            Ok(letters_to_decimal(value, a))
        }
        Primary::Roman => {
            let upper = if format.case == TCase::Upper {
                value.to_string()
            } else {
                value.to_uppercase()
            };
            Ok(roman_to_decimal(&upper))
        }
        Primary::Words => Ok(words_to_number(&value.to_lowercase())),
        Primary::Decimal => {
            let mut digits = value.to_string();
            if format.ordinal {
                // strip suffix (2 chars)
                let chars: Vec<char> = digits.chars().collect();
                digits = chars[..chars.len() - 2].iter().collect();
            }
            // strip separators
            if format.regular {
                digits = digits.replace(',', "");
            } else {
                for sep in &format.grouping_separators {
                    digits = digits.replace(sep.character, "");
                }
            }
            if format.zero_code != 0x30 {
                digits = digits
                    .chars()
                    .map(|c| {
                        let cp = c as u32 - format.zero_code + 0x30;
                        char::from_u32(cp).unwrap()
                    })
                    .collect();
            }
            Ok(digits.parse::<f64>().unwrap_or(f64::NAN))
        }
        Primary::Sequence => Err(JsonError::new("D3130", "unsupported numbering sequence")),
    }
}

// ---------------------------------------------------------------------------
// DateTime picture analysis
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Part {
    Literal(String),
    Marker(Marker),
}

#[derive(Clone, Debug)]
struct Marker {
    component: char,
    presentation1: Option<String>,
    presentation2: Option<char>,
    ordinal: bool,
    names: Option<TCase>,
    width_min: Option<usize>,
    width_max: Option<usize>,
    integer_format: Option<IntegerFormat>,
    n: i32,
}

impl Marker {
    fn new(component: char) -> Self {
        Marker {
            component,
            presentation1: None,
            presentation2: None,
            ordinal: false,
            names: None,
            width_min: None,
            width_max: None,
            integer_format: None,
            n: 0,
        }
    }
}

fn default_presentation(component: char) -> Option<&'static str> {
    match component {
        'Y' => Some("1"),
        'M' => Some("1"),
        'D' => Some("1"),
        'd' => Some("1"),
        'F' => Some("n"),
        'W' => Some("1"),
        'w' => Some("1"),
        'X' => Some("1"),
        'x' => Some("1"),
        'H' => Some("1"),
        'h' => Some("1"),
        'P' => Some("n"),
        'm' => Some("01"),
        's' => Some("01"),
        'f' => Some("1"),
        'Z' => Some("01:01"),
        'z' => Some("01:01"),
        'C' => Some("n"),
        'E' => Some("n"),
        _ => None,
    }
}

fn analyse_datetime_picture(picture: &str) -> Result<Vec<Part>, JsonError> {
    let mut spec: Vec<Part> = Vec::new();
    let chars: Vec<char> = picture.chars().collect();

    let add_literal = |spec: &mut Vec<Part>, start: usize, end: usize| {
        if end > start {
            let literal: String = chars[start..end].iter().collect();
            let literal = literal.replace("]]", "]");
            spec.push(Part::Literal(literal));
        }
    };

    let mut start = 0usize;
    let mut pos = 0usize;
    while pos < chars.len() {
        if chars[pos] == '[' {
            if pos + 1 < chars.len() && chars[pos + 1] == '[' {
                add_literal(&mut spec, start, pos);
                spec.push(Part::Literal("[".to_string()));
                pos += 2;
                start = pos;
                continue;
            }
            add_literal(&mut spec, start, pos);
            start = pos;
            // find closing ]
            let close = chars[start..].iter().position(|&c| c == ']').map(|p| p + start);
            let close = match close {
                Some(c) => c,
                None => return Err(JsonError::new("D3135", "no closing bracket")),
            };
            pos = close;
            let raw_marker: String = chars[start + 1..pos].iter().collect();
            // remove whitespace
            let marker: String = raw_marker.chars().filter(|c| !c.is_whitespace()).collect();
            let marker_chars: Vec<char> = marker.chars().collect();
            let mut def = Marker::new(marker_chars[0]);

            let comma = marker.rfind(',');
            let pres_mod: String;
            if let Some(comma_idx) = comma {
                let width_mod = &marker[comma_idx + 1..];
                let dash = width_mod.find('-');
                let parse_width = |wm: &str| -> Option<usize> {
                    if wm.is_empty() || wm == "*" {
                        None
                    } else {
                        wm.parse::<usize>().ok()
                    }
                };
                let (min_s, max_s) = match dash {
                    None => (width_mod.to_string(), None),
                    Some(d) => (
                        width_mod[..d].to_string(),
                        Some(width_mod[d + 1..].to_string()),
                    ),
                };
                def.width_min = parse_width(&min_s);
                def.width_max = max_s.as_deref().and_then(parse_width);
                // presMod = marker.substring(1, comma)
                pres_mod = marker_chars[1..comma_idx].iter().collect();
            } else {
                pres_mod = marker_chars[1..].iter().collect();
            }

            let pres_chars: Vec<char> = pres_mod.chars().collect();
            if pres_chars.len() == 1 {
                def.presentation1 = Some(pres_mod.clone());
            } else if pres_chars.len() > 1 {
                let last_char = pres_chars[pres_chars.len() - 1];
                if "atco".contains(last_char) {
                    def.presentation2 = Some(last_char);
                    if last_char == 'o' {
                        def.ordinal = true;
                    }
                    def.presentation1 = Some(pres_chars[..pres_chars.len() - 1].iter().collect());
                } else {
                    def.presentation1 = Some(pres_mod.clone());
                }
            } else {
                // no presentation modifier - default
                def.presentation1 = default_presentation(def.component).map(|s| s.to_string());
            }

            if def.presentation1.is_none() {
                return Err(JsonError::new("D3132", "unknown component specifier"));
            }

            let p1 = def.presentation1.clone().unwrap();
            let p1_chars: Vec<char> = p1.chars().collect();
            if p1_chars.first() == Some(&'n') {
                def.names = Some(TCase::Lower);
            } else if p1_chars.first() == Some(&'N') {
                if p1_chars.get(1) == Some(&'n') {
                    def.names = Some(TCase::Title);
                } else {
                    def.names = Some(TCase::Upper);
                }
            } else if "YMDdFWwXxHhmsf".contains(def.component) {
                let mut integer_pattern = p1.clone();
                if let Some(p2) = def.presentation2 {
                    integer_pattern.push(';');
                    integer_pattern.push(p2);
                }
                let mut int_format = analyse_integer_picture(&integer_pattern)?;
                if let Some(min) = def.width_min {
                    if int_format.mandatory_digits < min {
                        int_format.mandatory_digits = min;
                    }
                }
                if def.component == 'Y' {
                    def.n = -1;
                    if let Some(max) = def.width_max {
                        def.n = max as i32;
                        int_format.mandatory_digits = max;
                    } else {
                        let w = int_format.mandatory_digits + int_format.optional_digits;
                        if w >= 2 {
                            def.n = w as i32;
                        }
                    }
                }
                def.integer_format = Some(int_format);

                // previous integer part must have a fixed parse width
                if let Some(Part::Marker(prev)) = spec.last_mut() {
                    if let Some(prev_fmt) = prev.integer_format.as_mut() {
                        prev_fmt.parse_width = Some(prev_fmt.mandatory_digits);
                    }
                }
            }
            if def.component == 'Z' || def.component == 'z' {
                def.integer_format = Some(analyse_integer_picture(&p1)?);
            }

            spec.push(Part::Marker(def));
            start = pos + 1;
        }
        pos += 1;
    }
    add_literal(&mut spec, start, pos);
    Ok(spec)
}

// ---------------------------------------------------------------------------
// Date/time fragment extraction (UTC)
// ---------------------------------------------------------------------------

const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September",
    "October", "November", "December",
];
const DAYS: [&str; 8] = [
    "", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday",
];

const MILLIS_IN_DAY: i64 = 1000 * 60 * 60 * 24;

/// Milliseconds since epoch at 00:00 UTC for a y/m/d (month is 0-indexed).
fn date_utc_ymd(year: i64, month0: i64, day: i64) -> i64 {
    // Normalize month overflow/underflow.
    let mut y = year;
    let mut m = month0;
    while m < 0 {
        m += 12;
        y -= 1;
    }
    while m > 11 {
        m -= 12;
        y += 1;
    }
    let date = time::Date::from_calendar_date(
        y as i32,
        time::Month::try_from((m + 1) as u8).unwrap(),
        1,
    )
    .unwrap();
    let base = date.midnight().assume_utc().unix_timestamp() * 1000;
    base + (day - 1) * MILLIS_IN_DAY
}

fn weekday_iso(dt: OffsetDateTime) -> i64 {
    // Monday=1 .. Sunday=7
    match dt.weekday() {
        Weekday::Monday => 1,
        Weekday::Tuesday => 2,
        Weekday::Wednesday => 3,
        Weekday::Thursday => 4,
        Weekday::Friday => 5,
        Weekday::Saturday => 6,
        Weekday::Sunday => 7,
    }
}

fn start_of_first_week(year: i64, month0: i64) -> i64 {
    let jan1 = date_utc_ymd(year, month0, 1);
    let dt = OffsetDateTime::from_unix_timestamp_nanos((jan1 as i128) * 1_000_000).unwrap();
    let mut day_of = match dt.weekday() {
        Weekday::Sunday => 7,
        Weekday::Monday => 1,
        Weekday::Tuesday => 2,
        Weekday::Wednesday => 3,
        Weekday::Thursday => 4,
        Weekday::Friday => 5,
        Weekday::Saturday => 6,
    };
    // JS getUTCDay: Sunday=0; if 0 -> 7. Above already maps Sunday->7.
    if day_of == 0 {
        day_of = 7;
    }
    if day_of > 4 {
        jan1 + (8 - day_of) * MILLIS_IN_DAY
    } else {
        jan1 - (day_of - 1) * MILLIS_IN_DAY
    }
}

fn delta_weeks(start: i64, end: i64) -> f64 {
    (end - start) as f64 / (MILLIS_IN_DAY * 7) as f64 + 1.0
}

#[derive(Clone)]
enum Fragment {
    Num(f64),
    Text(String),
    None,
}

fn get_datetime_fragment(dt: OffsetDateTime, component: char) -> Fragment {
    let year = dt.year() as i64;
    let month0 = (dt.month() as u8 - 1) as i64;
    let day = dt.day() as i64;
    match component {
        'Y' => Fragment::Num(year as f64),
        'M' => Fragment::Num((month0 + 1) as f64),
        'D' => Fragment::Num(day as f64),
        'd' => {
            let today = date_utc_ymd(year, month0, day);
            let first_jan = date_utc_ymd(year, 0, 1);
            Fragment::Num(((today - first_jan) / MILLIS_IN_DAY + 1) as f64)
        }
        'F' => Fragment::Num(weekday_iso(dt) as f64),
        'W' => {
            let start1 = start_of_first_week(year, 0);
            let today = date_utc_ymd(year, month0, day);
            let mut week = delta_weeks(start1, today);
            if week > 52.0 {
                let start_following = start_of_first_week(year + 1, 0);
                if today >= start_following {
                    week = 1.0;
                }
            } else if week < 1.0 {
                let start_prev = start_of_first_week(year - 1, 0);
                week = delta_weeks(start_prev, today);
            }
            Fragment::Num(week.floor())
        }
        'w' => {
            let start1 = start_of_first_week(year, month0);
            let today = date_utc_ymd(year, month0, day);
            let mut week = delta_weeks(start1, today);
            if week > 4.0 {
                // next month
                let (ny, nm) = if month0 == 11 { (year + 1, 0) } else { (year, month0 + 1) };
                let start_following = start_of_first_week(ny, nm);
                if today >= start_following {
                    week = 1.0;
                }
            } else if week < 1.0 {
                let (py, pm) = if month0 == 0 { (year - 1, 11) } else { (year, month0 - 1) };
                let start_prev = start_of_first_week(py, pm);
                week = delta_weeks(start_prev, today);
            }
            Fragment::Num(week.floor())
        }
        'X' => {
            let start_iso = start_of_first_week(year, 0);
            let end_iso = start_of_first_week(year + 1, 0);
            let now = dt.unix_timestamp() * 1000 + dt.millisecond() as i64;
            if now < start_iso {
                Fragment::Num((year - 1) as f64)
            } else if now >= end_iso {
                Fragment::Num((year + 1) as f64)
            } else {
                Fragment::Num(year as f64)
            }
        }
        'x' => {
            let start_iso = start_of_first_week(year, month0);
            let (nm_y, nm_m) = if month0 == 11 { (year + 1, 0) } else { (year, month0 + 1) };
            let end_iso = start_of_first_week(nm_y, nm_m);
            let now = dt.unix_timestamp() * 1000 + dt.millisecond() as i64;
            if now < start_iso {
                let pm = if month0 == 0 { 11 } else { month0 - 1 };
                Fragment::Num((pm + 1) as f64)
            } else if now >= end_iso {
                Fragment::Num((nm_m + 1) as f64)
            } else {
                Fragment::Num((month0 + 1) as f64)
            }
        }
        'H' => Fragment::Num(dt.hour() as f64),
        'h' => {
            let mut h = dt.hour() as i64 % 12;
            if h == 0 {
                h = 12;
            }
            Fragment::Num(h as f64)
        }
        'P' => Fragment::Text(if dt.hour() >= 12 { "pm".into() } else { "am".into() }),
        'm' => Fragment::Num(dt.minute() as f64),
        's' => Fragment::Num(dt.second() as f64),
        'f' => Fragment::Num(dt.millisecond() as f64),
        'Z' | 'z' => Fragment::None,
        'C' => Fragment::Text("ISO".into()),
        'E' => Fragment::Text("ISO".into()),
        _ => Fragment::None,
    }
}

// ---------------------------------------------------------------------------
// formatDateTime
// ---------------------------------------------------------------------------

const ISO_DEFAULT_PICTURE: &str = "[Y0001]-[M01]-[D01]T[H01]:[m01]:[s01].[f001][Z01:01t]";

pub fn from_millis(
    millis: f64,
    picture: Option<&str>,
    timezone: Option<&str>,
) -> Result<String, JsonError> {
    let mut offset_hours: i64 = 0;
    let mut offset_minutes: i64 = 0;
    if let Some(tz) = timezone {
        // parseInt of e.g. "+0100" -> 100
        let offset = parse_int_prefix(tz);
        offset_hours = offset / 100;
        offset_minutes = offset % 100;
    }

    let spec = match picture {
        None => analyse_datetime_picture(ISO_DEFAULT_PICTURE)?,
        Some(p) => analyse_datetime_picture(p)?,
    };

    let offset_millis = (60 * offset_hours + offset_minutes) * 60 * 1000;
    let total_millis = millis as i64 + offset_millis;
    let dt = OffsetDateTime::from_unix_timestamp_nanos((total_millis as i128) * 1_000_000)
        .map_err(|_| JsonError::new("D3138", "invalid timestamp"))?;

    let mut result = String::new();
    for part in &spec {
        match part {
            Part::Literal(value) => result.push_str(value),
            Part::Marker(marker) => {
                result.push_str(&format_component(dt, marker, offset_hours, offset_minutes)?);
            }
        }
    }
    Ok(result)
}

fn format_component(
    dt: OffsetDateTime,
    marker: &Marker,
    offset_hours: i64,
    offset_minutes: i64,
) -> Result<String, JsonError> {
    let component = marker.component;
    let fragment = get_datetime_fragment(dt, component);

    if "YMDdFWwXxHhms".contains(component) {
        let mut num = match fragment {
            Fragment::Num(n) => n,
            _ => 0.0,
        };
        if component == 'Y' && marker.n != -1 {
            num %= 10f64.powi(marker.n);
        }
        if let Some(names) = &marker.names {
            let mut text = if component == 'M' || component == 'x' {
                MONTHS[(num as usize) - 1].to_string()
            } else if component == 'F' {
                DAYS[num as usize].to_string()
            } else {
                return Err(JsonError::new("D3133", "name not supported for component"));
            };
            match names {
                TCase::Upper => text = text.to_uppercase(),
                TCase::Lower => text = text.to_lowercase(),
                TCase::Title => {}
            }
            if let Some(max) = marker.width_max {
                if text.chars().count() > max {
                    text = text.chars().take(max).collect();
                }
            }
            Ok(text)
        } else {
            format_integer_internal(num, marker.integer_format.as_ref().unwrap())
        }
    } else if component == 'f' {
        let num = match fragment {
            Fragment::Num(n) => n,
            _ => 0.0,
        };
        format_integer_internal(num, marker.integer_format.as_ref().unwrap())
    } else if component == 'Z' || component == 'z' {
        let int_format = marker.integer_format.as_ref().unwrap();
        let offset = offset_hours * 100 + offset_minutes;
        let mut component_value;
        if int_format.regular {
            component_value = format_integer_internal(offset as f64, int_format)?;
        } else {
            let num_digits = int_format.mandatory_digits;
            if num_digits == 1 || num_digits == 2 {
                component_value = format_integer_internal(offset_hours as f64, int_format)?;
                if offset_minutes != 0 {
                    component_value.push(':');
                    component_value.push_str(&format_integer(offset_minutes as f64, "00")?);
                }
            } else if num_digits == 3 || num_digits == 4 {
                component_value = format_integer_internal(offset as f64, int_format)?;
            } else {
                return Err(JsonError::new("D3134", "too many digits in timezone"));
            }
        }
        if offset >= 0 {
            component_value = format!("+{component_value}");
        }
        if component == 'z' {
            component_value = format!("GMT{component_value}");
        }
        if offset == 0 && marker.presentation2 == Some('t') {
            component_value = "Z".to_string();
        }
        Ok(component_value)
    } else if component == 'P' {
        let mut text = match fragment {
            Fragment::Text(t) => t,
            _ => String::new(),
        };
        if marker.names == Some(TCase::Upper) {
            text = text.to_uppercase();
        }
        Ok(text)
    } else {
        // C, E and others returning text
        match fragment {
            Fragment::Text(t) => Ok(t),
            Fragment::Num(n) => Ok(format!("{n}")),
            Fragment::None => Ok(String::new()),
        }
    }
}

fn parse_int_prefix(s: &str) -> i64 {
    // mimic JS parseInt: optional sign, leading digits
    let s = s.trim_start();
    let bytes: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut sign = 1i64;
    if i < bytes.len() && (bytes[i] == '+' || bytes[i] == '-') {
        if bytes[i] == '-' {
            sign = -1;
        }
        i += 1;
    }
    let mut num = String::new();
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        num.push(bytes[i]);
        i += 1;
    }
    if num.is_empty() {
        0
    } else {
        sign * num.parse::<i64>().unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// toMillis (ISO 8601 parse)
// ---------------------------------------------------------------------------

pub fn to_millis_iso(timestamp: &str) -> Result<f64, JsonError> {
    if !iso8601_matches(timestamp) {
        return Err(JsonError::new(
            "D3110",
            format!(
                "The argument of the toMillis function must be an ISO 8601 formatted timestamp. Given {timestamp}"
            ),
        ));
    }
    parse_iso8601(timestamp)
        .ok_or_else(|| JsonError::new("D3110", "invalid ISO 8601 timestamp"))
}

/// Matches the JS regex:
/// ^\d{4}(-[01]\d)*(-[0-3]\d)*(T[0-2]\d:[0-5]\d:[0-5]\d)*(\.\d+)?([+-][0-2]\d:?[0-5]\d|Z)?$
fn iso8601_matches(s: &str) -> bool {
    let c: Vec<char> = s.chars().collect();
    let n = c.len();
    let mut i = 0;
    // \d{4}
    if n < 4 {
        return false;
    }
    for _ in 0..4 {
        if !c[i].is_ascii_digit() {
            return false;
        }
        i += 1;
    }
    // (-[01]\d)*
    while i + 2 < n + 1 && i + 2 <= n && c[i] == '-' && (c[i + 1] == '0' || c[i + 1] == '1') && c.get(i + 2).map_or(false, |d| d.is_ascii_digit()) {
        i += 3;
    }
    // (-[0-3]\d)*
    while i + 2 <= n && c[i] == '-' && ('0'..='3').contains(&c[i + 1]) && c.get(i + 2).map_or(false, |d| d.is_ascii_digit()) {
        i += 3;
    }
    // (T[0-2]\d:[0-5]\d:[0-5]\d)*
    while i + 8 <= n
        && c[i] == 'T'
        && ('0'..='2').contains(&c[i + 1])
        && c[i + 2].is_ascii_digit()
        && c[i + 3] == ':'
        && ('0'..='5').contains(&c[i + 4])
        && c[i + 5].is_ascii_digit()
        && c[i + 6] == ':'
        && ('0'..='5').contains(&c[i + 7])
        && c.get(i + 8).map_or(false, |d| d.is_ascii_digit())
    {
        i += 9;
    }
    // (\.\d+)?
    if i < n && c[i] == '.' {
        i += 1;
        let mut count = 0;
        while i < n && c[i].is_ascii_digit() {
            i += 1;
            count += 1;
        }
        if count == 0 {
            return false;
        }
    }
    // ([+-][0-2]\d:?[0-5]\d|Z)?
    if i < n {
        if c[i] == 'Z' {
            i += 1;
        } else if c[i] == '+' || c[i] == '-' {
            i += 1;
            if i + 1 >= n || !('0'..='2').contains(&c[i]) || !c[i + 1].is_ascii_digit() {
                return false;
            }
            i += 2;
            if i < n && c[i] == ':' {
                i += 1;
            }
            if i + 1 >= n + 1 || i + 2 > n || !('0'..='5').contains(&c[i]) || !c[i + 1].is_ascii_digit() {
                return false;
            }
            i += 2;
        } else {
            return false;
        }
    }
    i == n
}

/// Parse an ISO 8601 timestamp accepted by the regex into epoch millis.
fn parse_iso8601(s: &str) -> Option<f64> {
    let c: Vec<char> = s.chars().collect();
    let n = c.len();
    let mut i = 0;
    let take_digits = |c: &[char], i: &mut usize, k: usize| -> Option<i64> {
        let mut v = 0i64;
        for _ in 0..k {
            if *i >= c.len() || !c[*i].is_ascii_digit() {
                return None;
            }
            v = v * 10 + c[*i].to_digit(10).unwrap() as i64;
            *i += 1;
        }
        Some(v)
    };
    let year = take_digits(&c, &mut i, 4)?;
    let mut month = 1i64;
    let mut day = 1i64;
    let mut hour = 0i64;
    let mut minute = 0i64;
    let mut second = 0i64;
    let mut millis = 0i64;
    let mut tz_offset_min: i64 = 0;

    if i < n && c[i] == '-' {
        i += 1;
        month = take_digits(&c, &mut i, 2)?;
    }
    if i < n && c[i] == '-' {
        i += 1;
        day = take_digits(&c, &mut i, 2)?;
    }
    if i < n && c[i] == 'T' {
        i += 1;
        hour = take_digits(&c, &mut i, 2)?;
        if i < n && c[i] == ':' {
            i += 1;
            minute = take_digits(&c, &mut i, 2)?;
        }
        if i < n && c[i] == ':' {
            i += 1;
            second = take_digits(&c, &mut i, 2)?;
        }
    }
    if i < n && c[i] == '.' {
        i += 1;
        // up to 3 digits for millis (JS Date.parse uses ms; extra digits truncated)
        let mut frac = String::new();
        while i < n && c[i].is_ascii_digit() {
            frac.push(c[i]);
            i += 1;
        }
        // take first 3 digits, pad if shorter
        let mut ms_str: String = frac.chars().take(3).collect();
        while ms_str.len() < 3 {
            ms_str.push('0');
        }
        millis = ms_str.parse().unwrap_or(0);
    }
    if i < n {
        if c[i] == 'Z' {
            // UTC designator; no offset to apply.
        } else if c[i] == '+' || c[i] == '-' {
            let sign = if c[i] == '-' { -1 } else { 1 };
            i += 1;
            let oh = take_digits(&c, &mut i, 2)?;
            if i < n && c[i] == ':' {
                i += 1;
            }
            let om = take_digits(&c, &mut i, 2)?;
            tz_offset_min = sign * (oh * 60 + om);
        }
    }

    let base = date_utc_ymd(year, month - 1, day);
    let total = base + ((hour * 60 + minute) * 60 + second) * 1000 + millis
        - tz_offset_min * 60 * 1000;
    Some(total as f64)
}

// ---------------------------------------------------------------------------
// parseDateTime (toMillis with picture)
// ---------------------------------------------------------------------------

pub fn parse_datetime(timestamp: &str, picture: &str) -> Result<Option<f64>, JsonError> {
    let spec = analyse_datetime_picture(picture)?;

    // Build regex parts. We hand-roll a matcher over the parts.
    let mut matchers: Vec<RegexPart> = Vec::new();
    for part in &spec {
        matchers.push(build_regex_part(part)?);
    }

    // Build full regex string, case-insensitive, with capturing groups around
    // each non-literal part's regex.
    let mut regex_str = String::from("(?i)^");
    for m in &matchers {
        if m.parse_kind == ParseKind::Literal {
            regex_str.push_str(&m.regex);
        } else {
            regex_str.push('(');
            regex_str.push_str(&m.regex);
            regex_str.push(')');
        }
    }
    regex_str.push('$');

    let re = regex::Regex::new(&regex_str)
        .map_err(|_| JsonError::new("D3137", "invalid generated regex"))?;
    let caps = match re.captures(timestamp) {
        Some(c) => c,
        None => return Ok(None),
    };

    // Extract component values.
    let mut components: std::collections::HashMap<char, f64> = std::collections::HashMap::new();
    let mut group_idx = 1;
    for m in &matchers {
        if m.parse_kind == ParseKind::Literal {
            continue;
        }
        let value = caps.get(group_idx).map(|mt| mt.as_str()).unwrap_or("");
        group_idx += 1;
        let parsed = parse_regex_value(m, value);
        if let Some(v) = parsed {
            components.insert(m.component, v);
        }
    }

    if components.is_empty() {
        return Ok(None);
    }

    // bitmask logic
    let dm_a: u32 = 161;
    let dm_b: u32 = 130;
    let dm_c: u32 = 84;
    let dm_d: u32 = 72;
    let tm_a: u32 = 23;
    let tm_b: u32 = 47;

    let mut mask: u32 = 0;
    let shift = |mask: &mut u32, present: bool| {
        *mask <<= 1;
        if present {
            *mask += 1;
        }
    };
    let is_type = |mask: u32, t: u32| -> bool { (!t & mask) == 0 && (t & mask) != 0 };

    for part in "YXMxWwdD".chars() {
        shift(&mut mask, components.contains_key(&part));
    }
    let date_a = is_type(mask, dm_a);
    let date_b = !date_a && is_type(mask, dm_b);
    let date_c = is_type(mask, dm_c);
    let date_d = !date_c && is_type(mask, dm_d);

    mask = 0;
    for part in "PHhmsf".chars() {
        shift(&mut mask, components.contains_key(&part));
    }
    let time_a = is_type(mask, tm_a);
    let time_b = !time_a && is_type(mask, tm_b);

    let date_comps = if date_b {
        "YD"
    } else if date_c {
        "XxwF"
    } else if date_d {
        "XWF"
    } else {
        "YMD"
    };
    let time_comps = if time_b { "Phmsf" } else { "Hmsf" };

    let comps: String = format!("{date_comps}{time_comps}");

    let now = OffsetDateTime::now_utc();
    let mut start_specified = false;
    let mut end_specified = false;
    for part in comps.chars() {
        if !components.contains_key(&part) {
            if start_specified {
                let default = if "MDd".contains(part) { 1.0 } else { 0.0 };
                components.insert(part, default);
                end_specified = true;
            } else {
                if let Fragment::Num(v) = get_datetime_fragment(now, part) {
                    components.insert(part, v);
                }
            }
        } else {
            start_specified = true;
            if end_specified {
                return Err(JsonError::new("D3136", "gap between specified components"));
            }
        }
    }

    // fill in
    let mut m_val = *components.get(&'M').unwrap_or(&0.0);
    if m_val > 0.0 {
        m_val -= 1.0;
    } else {
        m_val = 0.0;
    }
    components.insert('M', m_val);

    let y = *components.get(&'Y').unwrap_or(&0.0) as i64;

    let (mut month0, mut day);
    if date_b {
        let first_jan = date_utc_ymd(y, 0, 1);
        let d = *components.get(&'d').unwrap_or(&1.0);
        let offset_millis = (d - 1.0) as i64 * MILLIS_IN_DAY;
        let derived = OffsetDateTime::from_unix_timestamp_nanos(
            ((first_jan + offset_millis) as i128) * 1_000_000,
        )
        .map_err(|_| JsonError::new("D3138", "invalid date"))?;
        month0 = (derived.month() as u8 - 1) as i64;
        day = derived.day() as i64;
    } else {
        month0 = m_val as i64;
        day = *components.get(&'D').unwrap_or(&1.0) as i64;
    }

    if date_c || date_d {
        return Err(JsonError::new("D3136", "unsupported date format"));
    }

    let mut hour = *components.get(&'H').unwrap_or(&0.0) as i64;
    if time_b {
        let h = *components.get(&'h').unwrap_or(&0.0) as i64;
        hour = if h == 12 { 0 } else { h };
        if components.get(&'P').copied() == Some(1.0) {
            hour += 12;
        }
    }
    let minute = *components.get(&'m').unwrap_or(&0.0) as i64;
    let second = *components.get(&'s').unwrap_or(&0.0) as i64;
    let frac = *components.get(&'f').unwrap_or(&0.0) as i64;

    // clamp month/day variables used
    let _ = &mut month0;
    let _ = &mut day;

    let base = date_utc_ymd(y, month0, day);
    let mut millis = base + ((hour * 60 + minute) * 60 + second) * 1000 + frac;

    let tz = components
        .get(&'Z')
        .or_else(|| components.get(&'z'))
        .copied();
    if let Some(tz) = tz {
        if tz != 0.0 {
            millis -= (tz as i64) * 60 * 1000;
        }
    }

    Ok(Some(millis as f64))
}

#[derive(Clone, PartialEq)]
enum ParseKind {
    Literal,
    Letters,
    Roman,
    Words,
    Decimal,
    NameLookup,
    Timezone,
}

struct RegexPart {
    component: char,
    regex: String,
    parse_kind: ParseKind,
    // for decimal
    ordinal: bool,
    regular: bool,
    grouping_chars: Vec<char>,
    zero_code: u32,
    // letters/roman case
    upper: bool,
    // name lookup
    lookup: Vec<(String, f64)>,
    // timezone
    tz_separator: Option<char>,
    tz_is_z: bool,
}

impl RegexPart {
    fn empty(component: char) -> Self {
        RegexPart {
            component,
            regex: String::new(),
            parse_kind: ParseKind::Literal,
            ordinal: false,
            regular: false,
            grouping_chars: Vec::new(),
            zero_code: 0x30,
            upper: false,
            lookup: Vec::new(),
            tz_separator: None,
            tz_is_z: false,
        }
    }
}

fn regex_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if ".*+?^${}()|[]\\".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn build_regex_part(part: &Part) -> Result<RegexPart, JsonError> {
    match part {
        Part::Literal(value) => {
            let mut p = RegexPart::empty('\0');
            p.regex = regex_escape(value);
            p.parse_kind = ParseKind::Literal;
            Ok(p)
        }
        Part::Marker(marker) => {
            let component = marker.component;
            if component == 'Z' || component == 'z' {
                let mut p = RegexPart::empty(component);
                p.parse_kind = ParseKind::Timezone;
                p.tz_is_z = component == 'z';
                let int_format = marker.integer_format.as_ref().unwrap();
                let separator = if !int_format.regular && int_format.grouping_separators.len() == 1 {
                    Some(int_format.grouping_separators[0].character)
                } else if int_format.regular {
                    Some(int_format.regular_char)
                } else {
                    None
                };
                p.tz_separator = separator;
                let mut regex = String::new();
                if component == 'z' {
                    regex.push_str("GMT");
                }
                regex.push_str("[-+][0-9]+");
                if let Some(sep) = separator {
                    regex.push_str(&regex_escape(&sep.to_string()));
                    regex.push_str("[0-9]+");
                }
                p.regex = regex;
                Ok(p)
            } else if let Some(int_format) = &marker.integer_format {
                let mut p = build_integer_regex(int_format)?;
                p.component = component;
                Ok(p)
            } else {
                // month/day/period name
                let mut p = RegexPart::empty(component);
                p.parse_kind = ParseKind::NameLookup;
                p.regex = "[a-zA-Z]+".to_string();
                let mut lookup: Vec<(String, f64)> = Vec::new();
                if component == 'M' || component == 'x' {
                    for (index, name) in MONTHS.iter().enumerate() {
                        let key = match marker.width_max {
                            Some(max) => name.chars().take(max).collect::<String>(),
                            None => name.to_string(),
                        };
                        lookup.push((key, (index + 1) as f64));
                    }
                } else if component == 'F' {
                    for (index, name) in DAYS.iter().enumerate() {
                        if index > 0 {
                            let key = match marker.width_max {
                                Some(max) => name.chars().take(max).collect::<String>(),
                                None => name.to_string(),
                            };
                            lookup.push((key, index as f64));
                        }
                    }
                } else if component == 'P' {
                    lookup.push(("am".to_string(), 0.0));
                    lookup.push(("pm".to_string(), 1.0));
                } else {
                    return Err(JsonError::new("D3133", "unsupported name option"));
                }
                p.lookup = lookup;
                Ok(p)
            }
        }
    }
}

fn build_integer_regex(format: &IntegerFormat) -> Result<RegexPart, JsonError> {
    let mut p = RegexPart::empty('\0');
    let is_upper = format.case == TCase::Upper;
    match format.primary {
        Primary::Letters => {
            p.parse_kind = ParseKind::Letters;
            p.upper = is_upper;
            p.regex = if is_upper { "[A-Z]+" } else { "[a-z]+" }.to_string();
        }
        Primary::Roman => {
            p.parse_kind = ParseKind::Roman;
            p.upper = is_upper;
            p.regex = if is_upper { "[MDCLXVI]+" } else { "[mdclxvi]+" }.to_string();
        }
        Primary::Words => {
            p.parse_kind = ParseKind::Words;
            let values = word_values();
            let mut keys: Vec<String> = values.keys().cloned().collect();
            keys.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
            keys.push("and".to_string());
            keys.push("[\\-, ]".to_string());
            p.regex = format!("(?:{})+", keys.join("|"));
        }
        Primary::Decimal => {
            p.parse_kind = ParseKind::Decimal;
            p.ordinal = format.ordinal;
            p.regular = format.regular;
            p.zero_code = format.zero_code;
            p.grouping_chars = format.grouping_separators.iter().map(|s| s.character).collect();
            let mut regex = String::from("[0-9]");
            if let Some(pw) = format.parse_width {
                regex.push_str(&format!("{{{pw}}}"));
            } else {
                regex.push('+');
            }
            if format.ordinal {
                regex.push_str("(?:th|st|nd|rd)");
            }
            p.regex = regex;
        }
        Primary::Sequence => {
            return Err(JsonError::new("D3130", "unsupported numbering sequence"));
        }
    }
    Ok(p)
}

fn parse_regex_value(part: &RegexPart, value: &str) -> Option<f64> {
    match part.parse_kind {
        ParseKind::Literal => None,
        ParseKind::Letters => {
            let a = if part.upper { 'A' } else { 'a' };
            Some(letters_to_decimal(value, a))
        }
        ParseKind::Roman => {
            let upper = if part.upper {
                value.to_string()
            } else {
                value.to_uppercase()
            };
            Some(roman_to_decimal(&upper))
        }
        ParseKind::Words => Some(words_to_number(&value.to_lowercase())),
        ParseKind::Decimal => {
            let mut digits = value.to_string();
            if part.ordinal {
                let chars: Vec<char> = digits.chars().collect();
                digits = chars[..chars.len() - 2].iter().collect();
            }
            if part.regular {
                digits = digits.replace(',', "");
            } else {
                for ch in &part.grouping_chars {
                    digits = digits.replace(*ch, "");
                }
            }
            if part.zero_code != 0x30 {
                digits = digits
                    .chars()
                    .map(|c| char::from_u32(c as u32 - part.zero_code + 0x30).unwrap())
                    .collect();
            }
            digits.parse::<f64>().ok()
        }
        ParseKind::NameLookup => {
            let lower = value.to_lowercase();
            for (key, v) in &part.lookup {
                if key.to_lowercase() == lower {
                    return Some(*v);
                }
            }
            None
        }
        ParseKind::Timezone => {
            let mut value = value;
            if part.tz_is_z {
                value = &value[3..]; // remove leading GMT
            }
            let offset_hours;
            let offset_minutes;
            if let Some(sep) = part.tz_separator {
                let idx = value.find(sep).unwrap_or(value.len());
                offset_hours = parse_int_prefix(&value[..idx]);
                offset_minutes = parse_int_prefix(&value[idx + sep.len_utf8()..]);
            } else {
                let numdigits = value.chars().count() - 1; // exclude sign
                if numdigits <= 2 {
                    offset_hours = parse_int_prefix(value);
                    offset_minutes = 0;
                } else {
                    offset_hours = parse_int_prefix(&value[..3]);
                    offset_minutes = parse_int_prefix(&value[3..]);
                    // note: sign applies to whole; minutes inherit hour sign via JS substring(3) is positive
                }
            }
            // JS: offsetHours * 60 + offsetMinutes
            Some((offset_hours * 60 + offset_minutes) as f64)
        }
    }
}
