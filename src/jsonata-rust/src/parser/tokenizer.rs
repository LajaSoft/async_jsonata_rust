//! Tokenizer translated from the upstream JSONata Pratt parser.
//!
//! This module mirrors the behaviour of `src/jsonata/src/parser.js` so the Rust parser can
//! consume the exact same token stream as the original JavaScript implementation.

use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Operator,
    Number,
    String,
    Regex,
    Name,
    Variable,
    Value,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenValue {
    None,
    Number(f64),
    String(String),
    Regex { pattern: String, flags: String },
    Boolean(bool),
    Null,
    Undefined,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub value: TokenValue,
    pub text: String,
    pub position: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TokenizerError {
    pub code: &'static str,
    pub position: usize,
    pub token: Option<String>,
}

pub struct Tokenizer<'a> {
    chars: Vec<char>,
    length: usize,
    position: usize,
    _input: &'a str,
}

impl<'a> Tokenizer<'a> {
    pub fn new(input: &'a str) -> Self {
        let chars: Vec<char> = input.chars().collect();
        let length = chars.len();
        Self {
            chars,
            length,
            position: 0,
            _input: input,
        }
    }

    fn current_char(&self) -> Option<char> {
        self.chars.get(self.position).copied()
    }

    fn peek_char(&self, offset: usize) -> Option<char> {
        self.chars.get(self.position + offset).copied()
    }

    fn peek_back(&self, offset: usize) -> Option<char> {
        if offset > self.position {
            None
        } else {
            self.chars.get(self.position - offset).copied()
        }
    }

    fn advance(&mut self, count: usize) {
        self.position = std::cmp::min(self.position + count, self.length);
    }

    fn slice(&self, start: usize, end: usize) -> String {
        self.chars[start..end].iter().collect()
    }

    fn create_token(
        &self,
        kind: TokenKind,
        value: TokenValue,
        text: String,
        _start: usize,
    ) -> Token {
        Token {
            kind,
            value,
            text,
            position: self.position,
        }
    }

    fn skip_whitespace(&mut self) {
        while matches!(
            self.current_char(),
            Some(' ' | '\t' | '\n' | '\r' | '\u{000B}')
        ) {
            self.advance(1);
        }
    }

    fn skip_comment(&mut self) -> Result<bool, TokenizerError> {
        if self.current_char() == Some('/') && self.peek_char(1) == Some('*') {
            let comment_start = self.position;
            self.advance(2);
            while let (Some(cur), Some(next)) = (self.current_char(), self.peek_char(1)) {
                if cur == '*' && next == '/' {
                    self.advance(2);
                    return Ok(true);
                }
                self.advance(1);
            }
            return Err(TokenizerError {
                code: "S0106",
                position: comment_start,
                token: None,
            });
        }
        Ok(false)
    }

    fn scan_regex(&mut self) -> Result<TokenValue, TokenizerError> {
        let pattern_start = self.position;
        let mut depth = 0usize;

        while self.position < self.length {
            let current = self.chars[self.position];
            if current == '/' && depth == 0 {
                let mut backslashes = 0usize;
                let mut idx = self.position;
                while idx > pattern_start && self.chars[idx - 1] == '\\' {
                    backslashes += 1;
                    idx -= 1;
                }
                if backslashes % 2 == 0 {
                    let pattern = self.slice(pattern_start, self.position);
                    if pattern.is_empty() {
                        return Err(TokenizerError {
                            code: "S0301",
                            position: self.position,
                            token: None,
                        });
                    }
                    self.advance(1); // consume '/'
                    let flag_start = self.position;
                    while matches!(self.current_char(), Some('i' | 'm')) {
                        self.advance(1);
                    }
                    let mut flags = self.slice(flag_start, self.position);
                    flags.push('g'); // JSONata always adds 'g'
                    return Ok(TokenValue::Regex { pattern, flags });
                }
            }

            match current {
                '(' | '[' | '{' => {
                    if self.peek_back(1) != Some('\\') {
                        depth += 1;
                    }
                }
                ')' | ']' | '}' => {
                    if self.peek_back(1) != Some('\\') && depth > 0 {
                        depth -= 1;
                    }
                }
                _ => {}
            }
            self.advance(1);
        }

        Err(TokenizerError {
            code: "S0302",
            position: self.position,
            token: None,
        })
    }

    pub fn next(&mut self, prefix: bool) -> Result<Option<Token>, TokenizerError> {
        loop {
            self.skip_whitespace();
            if self.position >= self.length {
                return Ok(None);
            }
            if !self.skip_comment()? {
                break;
            }
        }

        if !prefix && self.current_char() == Some('/') {
            let start = self.position;
            self.advance(1);
            let regex_value = self.scan_regex()?;
            let text = match &regex_value {
                TokenValue::Regex { pattern, flags } => format!("/{pattern}/{flags}"),
                _ => "/".to_string(),
            };
            return Ok(Some(self.create_token(TokenKind::Regex, regex_value, text, start)));
        }

        if let Some(token) = self.scan_operator() {
            return Ok(Some(token));
        }

        if let Some(ch) = self.current_char() {
            if ch == '"' || ch == '\'' {
                return self.scan_string(ch).map(Some);
            }
            if ch == '`' {
                return self.scan_backtick_name().map(Some);
            }
            if ch.is_ascii_digit()
                || (ch == '-' && self.peek_char(1).map_or(false, |c| c.is_ascii_digit()))
            {
                return self.scan_number().map(Some);
            }
            if ch == '$' {
                return self.scan_variable().map(Some);
            }
            return self.scan_name().map(Some);
        }

        Err(TokenizerError {
            code: "S0201",
            position: self.position,
            token: None,
        })
    }

    fn scan_operator(&mut self) -> Option<Token> {
        for (op, len) in [
            ("..", 2),
            (":=", 2),
            ("!=", 2),
            (">=", 2),
            ("<=", 2),
            ("**", 2),
            ("~>", 2),
            ("?:", 2),
            ("??", 2),
        ] {
            if self.peek_sequence(op) {
                let start = self.position;
                self.advance(len);
                return Some(self.create_token(
                    TokenKind::Operator,
                    TokenValue::String(op.to_string()),
                    op.to_string(),
                    start,
                ));
            }
        }

        if let Some(ch) = self.current_char() {
            if is_single_char_operator(ch) {
                let start = self.position;
                self.advance(1);
                return Some(self.create_token(
                    TokenKind::Operator,
                    TokenValue::String(ch.to_string()),
                    ch.to_string(),
                    start,
                ));
            }
        }

        None
    }

    fn peek_sequence(&self, seq: &str) -> bool {
        if self.position + seq.len() > self.length {
            return false;
        }
        self.chars[self.position..self.position + seq.len()]
            .iter()
            .collect::<String>()
            == seq
    }

    fn scan_string(&mut self, quote: char) -> Result<Token, TokenizerError> {
        let token_start = self.position;
        self.advance(1); // skip opening quote
        let mut value = String::new();

        while self.position < self.length {
            let current = self.current_char().ok_or(TokenizerError {
                code: "S0101",
                position: token_start,
                token: None,
            })?;
            if current == quote {
                self.advance(1);
                let text = value.clone();
                return Ok(self.create_token(
                    TokenKind::String,
                    TokenValue::String(value),
                    text,
                    token_start,
                ));
            }
            if current == '\\' {
                self.advance(1);
                let escaped = self.current_char().ok_or(TokenizerError {
                    code: "S0103",
                    position: self.position,
                    token: None,
                })?;
                let decoded = match escaped {
                    '"' => '"',
                    '\'' => '\'',
                    '\\' => '\\',
                    '/' => '/',
                    'b' => '\u{0008}',
                    'f' => '\u{000C}',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    'u' => {
                        let start = self.position + 1;
                        let end = start + 4;
                        if end > self.length {
                            return Err(TokenizerError {
                                code: "S0104",
                                position: self.position,
                                token: None,
                            });
                        }
                        let octets = self.slice(start, end);
                        if !octets.chars().all(|c| c.is_ascii_hexdigit()) {
                            return Err(TokenizerError {
                                code: "S0104",
                                position: self.position,
                                token: Some(octets),
                            });
                        }
                        let codepoint =
                            u32::from_str_radix(&octets, 16).map_err(|_| TokenizerError {
                                code: "S0104",
                                position: self.position,
                                token: None,
                            })?;
                        self.position += 4;
                        char::from_u32(codepoint).ok_or(TokenizerError {
                            code: "S0104",
                            position: self.position,
                            token: None,
                        })?
                    }
                    other => {
                        return Err(TokenizerError {
                            code: "S0103",
                            position: self.position,
                            token: Some(other.to_string()),
                        })
                    }
                };
                value.push(decoded);
            } else {
                value.push(current);
            }
            self.advance(1);
        }

        Err(TokenizerError {
            code: "S0101",
            position: token_start,
            token: None,
        })
    }

    fn scan_backtick_name(&mut self) -> Result<Token, TokenizerError> {
        self.advance(1); // skip opening backtick
        let start = self.position;
        while self.position < self.length {
            if self.current_char() == Some('`') {
                let name = self.slice(start, self.position);
                self.advance(1);
                return Ok(self.create_token(
                    TokenKind::Name,
                    TokenValue::String(name.clone()),
                    name,
                    start - 1,
                ));
            }
            self.advance(1);
        }

        Err(TokenizerError {
            code: "S0105",
            position: start,
            token: None,
        })
    }

    fn scan_number(&mut self) -> Result<Token, TokenizerError> {
        let start = self.position;
        if self.current_char() == Some('-') {
            self.advance(1);
        }
        while matches!(self.current_char(), Some(c) if c.is_ascii_digit()) {
            self.advance(1);
        }
        if self.current_char() == Some('.')
            && self.peek_char(1).map_or(false, |c| c.is_ascii_digit())
        {
            self.advance(1);
            while matches!(self.current_char(), Some(c) if c.is_ascii_digit()) {
                self.advance(1);
            }
        }
        if matches!(self.current_char(), Some('e' | 'E')) {
            self.advance(1);
            if matches!(self.current_char(), Some('+' | '-')) {
                self.advance(1);
            }
            while matches!(self.current_char(), Some(c) if c.is_ascii_digit()) {
                self.advance(1);
            }
        }
        let text = self.slice(start, self.position);
        let number = f64::from_str(&text).map_err(|_| TokenizerError {
            code: "S0102",
            position: start,
            token: Some(text.clone()),
        })?;
        Ok(self.create_token(TokenKind::Number, TokenValue::Number(number), text, start))
    }

    fn scan_variable(&mut self) -> Result<Token, TokenizerError> {
        let start = self.position;
        self.advance(1); // skip '$'
        while matches!(self.current_char(), Some(c) if is_name_char(c)) {
            self.advance(1);
        }
        let name = self.slice(start + 1, self.position);
        Ok(self.create_token(
            TokenKind::Variable,
            TokenValue::String(name.clone()),
            format!("${name}"),
            start,
        ))
    }

    fn scan_name(&mut self) -> Result<Token, TokenizerError> {
        let start = self.position;
        while matches!(self.current_char(), Some(c) if is_name_char(c)) {
            self.advance(1);
        }
        let name = self.slice(start, self.position);
        if name.is_empty() {
            return Err(TokenizerError {
                code: "S0201",
                position: start,
                token: None,
            });
        }

        let (kind, value) = match name.as_str() {
            "or" | "and" | "in" => (TokenKind::Operator, TokenValue::String(name.clone())),
            "true" => (TokenKind::Value, TokenValue::Boolean(true)),
            "false" => (TokenKind::Value, TokenValue::Boolean(false)),
            "null" => (TokenKind::Value, TokenValue::Null),
            "undefined" => (TokenKind::Value, TokenValue::Undefined),
            _ => (TokenKind::Name, TokenValue::String(name.clone())),
        };

        Ok(self.create_token(kind, value, name, start))
    }
}

fn is_single_char_operator(ch: char) -> bool {
    matches!(
        ch,
        '.' | '['
            | ']'
            | '{'
            | '}'
            | '('
            | ')'
            | ','
            | '@'
            | '#'
            | ';'
            | ':'
            | '+'
            | '-'
            | '*'
            | '/'
            | '%'
            | '<'
            | '>'
            | '='
            | '!'
            | '&'
            | '|'
            | '^'
            | '~'
            | '?'
    )
}

fn is_name_char(ch: char) -> bool {
    !(ch.is_whitespace() || is_single_char_operator(ch) || matches!(ch, '"' | '\'' | '`'))
}

#[cfg(test)]
mod tests {
    use super::{TokenKind, TokenValue, Tokenizer};

    #[test]
    fn tokenizes_range_without_swallowing_first_dot() {
        let mut tokenizer = Tokenizer::new("1..10000");

        let first = tokenizer.next(false).expect("first token").expect("first token exists");
        assert_eq!(first.kind, TokenKind::Number);
        assert_eq!(first.value, TokenValue::Number(1.0));
        assert_eq!(first.text, "1");

        let second = tokenizer.next(false).expect("second token").expect("second token exists");
        assert_eq!(second.kind, TokenKind::Operator);
        assert_eq!(second.value, TokenValue::String("..".to_string()));
        assert_eq!(second.text, "..");

        let third = tokenizer.next(false).expect("third token").expect("third token exists");
        assert_eq!(third.kind, TokenKind::Number);
        assert_eq!(third.value, TokenValue::Number(10000.0));
        assert_eq!(third.text, "10000");
    }

    #[test]
    fn tokenizes_decimal_numbers() {
        let mut tokenizer = Tokenizer::new("1.5");
        let token = tokenizer
            .next(false)
            .expect("decimal token")
            .expect("decimal token exists");
        assert_eq!(token.kind, TokenKind::Number);
        assert_eq!(token.value, TokenValue::Number(1.5));
        assert_eq!(token.text, "1.5");
    }
}
