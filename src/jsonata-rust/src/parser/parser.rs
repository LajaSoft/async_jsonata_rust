use std::collections::HashMap;

use serde_json::{json, Value};

use super::ast::AstNode;
use super::error::ParserError;
use super::tokenizer::{TokenKind, TokenValue, Tokenizer};

#[derive(Debug, Clone)]
struct TokenData {
    id: String,
    token_type: String,
    value: Value,
    position: usize,
}

pub struct Parser<'a> {
    source: &'a str,
    tokenizer: Tokenizer<'a>,
    operators: HashMap<String, u32>,
    current: Option<TokenData>,
    _recover: bool,
    pub errors: Vec<ParserError>,
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str, recover: bool) -> Result<Self, ParserError> {
        let mut parser = Self {
            source,
            tokenizer: Tokenizer::new(source),
            operators: operator_table(),
            current: None,
            _recover: recover,
            errors: Vec::new(),
        };
        parser.advance(false)?;
        Ok(parser)
    }

    pub fn parse(mut self) -> Result<Value, ParserError> {
        let expr = self.expression(0)?;
        if let Some(current) = &self.current {
            if current.id != "(end)" {
                return Err(ParserError::new("S0201", current.position)
                    .with_token(current.value.clone()));
            }
        }
        let processed = process_ast(expr.into())?;
        let processed = annotate_parent_references(processed)?;
        if processed
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|expr_type| expr_type == "parent")
            || processed.get("seekingParent").is_some()
        {
            let token = processed
                .get("type")
                .cloned()
                .unwrap_or(Value::Null);
            return Err(
                ParserError::new("S0217", expr_position(&processed))
                    .with_token(token),
            );
        }
        Ok(processed)
    }

    fn expression(&mut self, rbp: u32) -> Result<AstNode, ParserError> {
        let token = self
            .current
            .clone()
            .ok_or_else(|| ParserError::new("S0201", self.source.len()))?;
        self.advance(true)?;
        let mut left = self.nud(token)?;
        while let Some(current) = &self.current {
            let lbp = self.binding_power(&current.id);
            if rbp >= lbp {
                break;
            }
            let token = current.clone();
            self.advance(true)?;
            left = self.led(token, left)?;
        }
        Ok(left)
    }

    fn nud(&mut self, token: TokenData) -> Result<AstNode, ParserError> {
        match token.token_type.as_str() {
            "name" | "number" | "string" | "value" | "regex" | "literal" | "variable" => {
                return Ok(AstNode::new(
                    token.id,
                    token.token_type,
                    token.value,
                    token.position,
                ));
            }
            _ => {}
        }

        match token.id.as_str() {
            "(" => self.parse_parenthesized(token),
            "[" => self.parse_array(token),
            "{" => self.parse_object(token),
            "-" => {
                let mut node =
                    AstNode::new(token.id, token.token_type, token.value, token.position);
                node.set_type("unary");
                let expr = self.expression(70)?;
                node.set_node("expression", expr);
                Ok(node)
            }
            "*" => {
                let mut node =
                    AstNode::new(token.id, token.token_type, token.value, token.position);
                node.set_type("wildcard");
                Ok(node)
            }
            "**" => {
                let mut node =
                    AstNode::new(token.id, token.token_type, token.value, token.position);
                node.set_type("descendant");
                Ok(node)
            }
            "%" => {
                let mut node =
                    AstNode::new(token.id, token.token_type, token.value, token.position);
                node.set_type("parent");
                Ok(node)
            }
            "|" => self.parse_transform(token),
            _ => {
                let code = if token.id == "(end)" { "S0207" } else { "S0211" };
                Err(ParserError::new(code, token.position)
                    .with_token(token.value.clone()))
            }
        }
    }

    fn led(&mut self, token: TokenData, left: AstNode) -> Result<AstNode, ParserError> {
        match token.id.as_str() {
            "." => self.parse_dot(token, left),
            "[" => self.parse_filter(token, left),
            "(" => self.parse_function_call(token, left),
            "{" => self.parse_object_group(token, left),
            "^" => self.parse_order_by(token, left),
            ":" => Err(ParserError::new("S0201", token.position)),
            "+" | "-" | "*" | "/" | "%" | "=" | "<" | ">" | "!=" | "<=" | ">=" | "&" | "and"
            | "or" | "in" | "~>" => self.parse_binary(token, left),
            "??" => self.parse_coalesce(token, left),
            "?" => self.parse_ternary(token, left),
            "?:" => self.parse_default(token, left),
            ":=" => self.parse_assignment(token, left),
            "@"
            | "#"
            | ".."
            | ";" => self.parse_binary(token, left),
            _ => Err(ParserError::new("S0201", token.position)),
        }
    }

    fn binding_power(&self, id: &str) -> u32 {
        self.operators.get(id).copied().unwrap_or(0)
    }

    fn advance(&mut self, infix: bool) -> Result<(), ParserError> {
        let token = match self.next_token(infix)? {
            Some(token) => token,
            None => TokenData {
                id: "(end)".to_string(),
                token_type: "end".to_string(),
                value: Value::Null,
                position: self.source.len(),
            },
        };
        self.current = Some(token);
        Ok(())
    }

    fn next_token(&mut self, infix: bool) -> Result<Option<TokenData>, ParserError> {
        let token = self.tokenizer.next(infix).map_err(|err| {
            ParserError::new(err.code, err.position).with_token(
                err.token
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            )
        })?;
        Ok(token.map(|t| TokenData {
            id: t.text.clone(),
            token_type: match t.kind {
                TokenKind::Operator => "operator".to_string(),
                TokenKind::Number => "number".to_string(),
                TokenKind::String => "string".to_string(),
                TokenKind::Regex => "regex".to_string(),
                TokenKind::Name => "name".to_string(),
                TokenKind::Variable => "variable".to_string(),
                TokenKind::Value => "value".to_string(),
                TokenKind::Eof => "end".to_string(),
            },
            value: token_value_to_json(&t.value),
            position: t.position,
        }))
    }

    fn parse_parenthesized(&mut self, token: TokenData) -> Result<AstNode, ParserError> {
        let mut expressions = Vec::new();
        while let Some(current) = &self.current {
            if current.id == ")" {
                break;
            }
            expressions.push(self.expression(0)?);
            if let Some(current) = &self.current {
                if current.id != ";" {
                    break;
                }
                self.advance(false)?;
            }
        }
        self.expect(")", true)?;
        let mut node =
            AstNode::new(token.id, token.token_type, token.value, token.position);
        node.set_type("block");
        node.set_field(
            "expressions",
            Value::Array(expressions.into_iter().map(Into::into).collect()),
        );
        Ok(node)
    }

    fn parse_array(&mut self, token: TokenData) -> Result<AstNode, ParserError> {
        let mut items = Vec::new();
        while let Some(current) = &self.current {
            if current.id == "]" {
                break;
            }
            let mut item = self.expression(0)?;
            if let Some(current) = &self.current {
                if current.id == ".." {
                    let mut range =
                        AstNode::new("..".to_string(), "operator".to_string(), Value::String("..".into()), current.position);
                    self.advance(false)?;
                    range.set_type("binary");
                    range.set_node("lhs", item);
                    let rhs = self.expression(0)?;
                    range.set_node("rhs", rhs);
                    item = range;
                }
            }
            items.push(item);
            if let Some(current) = &self.current {
                if current.id != "," {
                    break;
                }
                self.advance(false)?;
            }
        }
        self.expect("]", true)?;
        let mut node =
            AstNode::new(token.id, token.token_type, token.value, token.position);
        node.set_type("unary");
        node.set_field(
            "expressions",
            Value::Array(items.into_iter().map(Into::into).collect()),
        );
        Ok(node)
    }

    fn parse_object(&mut self, token: TokenData) -> Result<AstNode, ParserError> {
        let pairs = self.parse_object_pairs()?;
        self.expect("}", true)?;
        let mut node =
            AstNode::new(token.id, token.token_type, token.value, token.position);
        node.set_type("unary");
        let fields = pairs
            .into_iter()
            .map(|(k, v)| Value::Array(vec![k.into(), v.into()]))
            .collect::<Vec<_>>();
        node.set_field("lhs", Value::Array(fields));
        Ok(node)
    }

    fn parse_object_group(
        &mut self,
        token: TokenData,
        left: AstNode,
    ) -> Result<AstNode, ParserError> {
        let pairs = self.parse_object_pairs()?;
        self.expect("}", true)?;

        let mut node =
            AstNode::new(token.id, token.token_type, token.value, token.position);
        node.set_type("binary");
        node.set_node("lhs", left);
        let fields = pairs
            .into_iter()
            .map(|(k, v)| Value::Array(vec![k.into(), v.into()]))
            .collect::<Vec<_>>();
        node.set_field("rhs", Value::Array(fields));
        Ok(node)
    }

    fn parse_object_pairs(&mut self) -> Result<Vec<(AstNode, AstNode)>, ParserError> {
        let mut pairs = Vec::new();
        while let Some(current) = &self.current {
            if current.id == "}" {
                break;
            }
            let key = self.expression(0)?;
            self.expect(":", false)?;
            let value = self.expression(0)?;
            pairs.push((key, value));
            if let Some(current) = &self.current {
                if current.id != "," {
                    break;
                }
                self.advance(false)?;
            }
        }

        Ok(pairs)
    }

    fn parse_transform(&mut self, token: TokenData) -> Result<AstNode, ParserError> {
        let pattern = self.expression(0)?;
        self.expect("|", false)?;
        let update = self.expression(0)?;
        let mut delete = None;
        if let Some(current) = &self.current {
            if current.id == "," {
                self.advance(false)?;
                delete = Some(self.expression(0)?);
            }
        }
        self.expect("|", false)?;
        let mut node =
            AstNode::new(token.id, token.token_type, token.value, token.position);
        node.set_type("transform");
        node.set_node("pattern", pattern);
        node.set_node("update", update);
        if let Some(del) = delete {
            node.set_node("delete", del);
        }
        Ok(node)
    }

    fn parse_dot(&mut self, token: TokenData, left: AstNode) -> Result<AstNode, ParserError> {
        let mut node =
            AstNode::new(token.id, token.token_type, token.value, token.position);
        node.set_type("binary");
        node.set_node("lhs", left);
        let right = self.expression(self.binding_power("."))?;
        node.set_node("rhs", right);
        Ok(node)
    }

    fn parse_filter(&mut self, token: TokenData, left: AstNode) -> Result<AstNode, ParserError> {
        if let Some(current) = &self.current {
            if current.id == "]" {
                // keep array
                self.advance(false)?;
                let mut left = left;
                left.set_field("keepArray", Value::Bool(true));
                return Ok(left);
            }
        }
        let mut node =
            AstNode::new(token.id, token.token_type, token.value, token.position);
        node.set_type("binary");
        node.set_node("lhs", left);
        let right = self.expression(self.binding_power("]"))?;
        node.set_node("rhs", right);
        self.expect("]", true)?;
        Ok(node)
    }

    fn parse_function_call(
        &mut self,
        token: TokenData,
        left: AstNode,
    ) -> Result<AstNode, ParserError> {
        let mut args: Vec<AstNode> = Vec::new();
        while let Some(current) = &self.current {
            if current.id == ")" {
                break;
            }
            if current.id == "?" {
                let question = current.clone();
                self.advance(false)?;
                let mut arg = AstNode::new(
                    question.id,
                    question.token_type,
                    question.value,
                    question.position,
                );
                arg.set_type("operator");
                args.push(arg);
            } else {
                args.push(self.expression(0)?);
            }
            if let Some(current) = &self.current {
                if current.id != "," {
                    break;
                }
                self.advance(false)?;
            }
        }
        self.expect(")", true)?;

        let arguments_value: Value =
            Value::Array(args.iter().cloned().map(Into::into).collect());

        if is_lambda_name(&left) {
            for (index, arg) in args.iter().enumerate() {
                if arg.node_type != "variable" {
                    return Err(ParserError::new("S0208", arg.position)
                        .with_token(arg.value.clone())
                        .with_value(json!((index + 1) as u64)));
                }
            }

            let mut lambda =
                AstNode::new(left.id.clone(), left.token_type.clone(), left.value.clone(), left.position);
            lambda.set_type("lambda");
            lambda.set_field("arguments", arguments_value);

            if let Some(current) = &self.current {
                if current.id == "<" {
                    let signature = self.parse_signature()?; // placeholder will return None until implemented
                    if let Some(sig) = signature {
                        lambda.set_field("signature", sig);
                    }
                }
            }

            self.expect("{", false)?;
            let body = self.expression(0)?;
            lambda.set_node("body", body);
            self.expect("}", false)?;
            return Ok(lambda);
        }

        let mut node =
            AstNode::new(token.id, token.token_type, token.value, token.position);
        node.set_type("function");
        node.set_node("procedure", left);
        node.set_field("arguments", arguments_value);
        Ok(node)
    }

    fn parse_order_by(&mut self, token: TokenData, left: AstNode) -> Result<AstNode, ParserError> {
        self.expect("(", false)?;
        let mut terms = Vec::new();
        loop {
            let mut term = json!({"descending": false});
            if let Some(current) = &self.current {
                if current.id == "<" {
                    self.advance(false)?;
                } else if current.id == ">" {
                    term["descending"] = Value::Bool(true);
                    self.advance(false)?;
                }
            }
            term["expression"] = self.expression(0)?.into();
            if let Some(current) = &self.current {
                if current.id == "," {
                    self.advance(false)?;
                    terms.push(term);
                    continue;
                }
            }
            terms.push(term);
            break;
        }
        self.expect(")", true)?;
        let mut node = AstNode::new(
            token.id.clone(),
            "operator".to_string(),
            Value::String(token.id),
            token.position,
        );
        node.set_type("binary");
        node.set_node("lhs", left);
        node.set_field("rhs", Value::Array(terms));
        Ok(node)
    }

    fn parse_binary(&mut self, token: TokenData, left: AstNode) -> Result<AstNode, ParserError> {
        let mut node =
            AstNode::new(token.id.clone(), "operator".to_string(), token.value.clone(), token.position);
        node.set_type("binary");
        node.set_node("lhs", left);
        let rhs = self.expression(self.binding_power(&token.id))?;
        node.set_node("rhs", rhs);
        node.set_value(Value::String(token.id));
        Ok(node)
    }

    fn parse_coalesce(&mut self, token: TokenData, left: AstNode) -> Result<AstNode, ParserError> {
        let mut node = AstNode::new(
            token.id,
            token.token_type,
            token.value,
            token.position,
        );
        node.set_type("condition");
        let mut condition = AstNode::new(
            "(".to_string(),
            "operator".to_string(),
            Value::String("(".to_string()),
            left.position,
        );
        condition.set_type("function");
        let mut exists_proc = AstNode::new(
            "exists".to_string(),
            "variable".to_string(),
            Value::String("exists".to_string()),
            left.position,
        );
        exists_proc.set_type("variable");
        condition.set_node("procedure", exists_proc);
        condition.set_field("arguments", Value::Array(vec![left.clone().into()]));
        node.set_node("condition", condition);
        node.set_node("then", left);
        let else_branch = self.expression(0)?;
        node.set_node("else", else_branch);
        Ok(node)
    }

    fn parse_ternary(&mut self, token: TokenData, left: AstNode) -> Result<AstNode, ParserError> {
        let mut node =
            AstNode::new(token.id, token.token_type, token.value, token.position);
        node.set_type("condition");
        node.set_node("condition", left);
        let then_branch = self.expression(0)?;
        node.set_node("then", then_branch);
        if let Some(current) = &self.current {
            if current.id == ":" {
                self.advance(false)?;
                let else_branch = self.expression(0)?;
                node.set_node("else", else_branch);
            }
        }
        Ok(node)
    }

    fn parse_default(&mut self, token: TokenData, left: AstNode) -> Result<AstNode, ParserError> {
        let mut node =
            AstNode::new(token.id, token.token_type, token.value, token.position);
        node.set_type("condition");
        node.set_node("condition", left.clone());
        node.set_node("then", left);
        let else_branch = self.expression(0)?;
        node.set_node("else", else_branch);
        Ok(node)
    }

    fn parse_assignment(
        &mut self,
        token: TokenData,
        left: AstNode,
    ) -> Result<AstNode, ParserError> {
        if left.node_type != "variable" {
            return Err(ParserError::new("S0212", left.position).with_token(left.value.clone()));
        }

        let mut node = AstNode::new(
            token.id.clone(),
            "operator".to_string(),
            Value::String(token.id),
            token.position,
        );
        node.set_type("binary");
        node.set_node("lhs", left);
        let rhs = self.expression(self.binding_power(":=").saturating_sub(1))?;
        node.set_node("rhs", rhs);
        Ok(node)
    }

    fn parse_signature(&mut self) -> Result<Option<Value>, ParserError> {
        if !matches!(self.current.as_ref().map(|t| t.id.as_str()), Some("<")) {
            return Ok(None);
        }

        let signature_start = self
            .current
            .as_ref()
            .map(|token| token.position)
            .unwrap_or(self.source.len());
        let mut signature = String::new();
        let mut depth = 0usize;

        loop {
            let token = self
                .current
                .clone()
                .ok_or_else(|| ParserError::new("S0206", self.source.len()))?;

            if token.id == "(end)" {
                return Err(ParserError::new("S0203", token.position));
            }

            if token.id == "<" {
                depth += 1;
            } else if token.id == ">" {
                if depth == 0 {
                    return Err(ParserError::new("S0206", token.position));
                }
                depth -= 1;
            }

            signature.push_str(&token.id);
            self.advance(false)?;

            if depth == 0 {
                break;
            }
        }

        if let Err(err) = validate_signature_definition(&signature) {
            let mut parser_error = ParserError::new(err.code, signature_start + err.offset);
            if let Some(value) = err.value {
                parser_error = parser_error.with_value(value);
            }
            return Err(parser_error);
        }

        Ok(Some(Value::String(signature)))
    }

    fn expect(&mut self, id: &str, infix: bool) -> Result<(), ParserError> {
        if let Some(current) = &self.current {
            if current.id != id {
                return Err(ParserError::new("S0202", current.position)
                    .with_token(current.value.clone())
                    .with_value(Value::String(id.to_string())));
            }
        } else {
            return Err(ParserError::new("S0203", self.source.len()));
        }
        self.advance(infix)
    }
}

fn is_lambda_name(node: &AstNode) -> bool {
    node.node_type == "name" && (node.id == "function" || node.id == "\u{03BB}")
}

struct SignatureValidationError {
    code: &'static str,
    offset: usize,
    value: Option<Value>,
}

fn validate_signature_definition(signature: &str) -> Result<(), SignatureValidationError> {
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

fn token_value_to_json(value: &TokenValue) -> Value {
    match value {
        TokenValue::None | TokenValue::Undefined | TokenValue::Null => Value::Null,
        TokenValue::Number(n) => json!(n),
        TokenValue::String(s) => Value::String(s.clone()),
        TokenValue::Regex { pattern, flags } => json!({ "pattern": pattern, "flags": flags }),
        TokenValue::Boolean(b) => Value::Bool(*b),
    }
}

fn operator_table() -> HashMap<String, u32> {
    let mut map = HashMap::new();
    map.insert(".".to_string(), 75);
    map.insert("[".to_string(), 80);
    map.insert("]".to_string(), 0);
    map.insert("{".to_string(), 70);
    map.insert("}".to_string(), 0);
    map.insert("(".to_string(), 80);
    map.insert(")".to_string(), 0);
    map.insert(",".to_string(), 0);
    map.insert("@".to_string(), 80);
    map.insert("#".to_string(), 80);
    map.insert(";".to_string(), 0);
    map.insert(":".to_string(), 0);
    map.insert("?".to_string(), 20);
    map.insert("+".to_string(), 50);
    map.insert("-".to_string(), 50);
    map.insert("*".to_string(), 60);
    map.insert("/".to_string(), 60);
    map.insert("%".to_string(), 60);
    map.insert("|".to_string(), 0);
    map.insert("=".to_string(), 40);
    map.insert("<".to_string(), 40);
    map.insert(">".to_string(), 40);
    map.insert("^".to_string(), 40);
    map.insert("**".to_string(), 60);
    map.insert("..".to_string(), 0);
    map.insert(":=".to_string(), 10);
    map.insert("!=".to_string(), 40);
    map.insert("<=".to_string(), 40);
    map.insert(">=".to_string(), 40);
    map.insert("~>".to_string(), 40);
    map.insert("?:".to_string(), 40);
    map.insert("??".to_string(), 40);
    map.insert("and".to_string(), 30);
    map.insert("or".to_string(), 25);
    map.insert("in".to_string(), 40);
    map.insert("&".to_string(), 50);
    map
}

fn process_ast(expr: Value) -> Result<Value, ParserError> {
    let keep_array = expr
        .get("keepArray")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut result = match expr {
        Value::Object(map) => process_ast_object(map)?,
        other => other,
    };

    if keep_array {
        if let Some(result_map) = result.as_object_mut() {
            result_map.insert("keepArray".to_string(), Value::Bool(true));
        }
    }

    Ok(result)
}

#[derive(Default)]
struct ParentReferenceState {
    next_slot_index: usize,
    slot_label_aliases: HashMap<usize, String>,
}

fn annotate_parent_references(expr: Value) -> Result<Value, ParserError> {
    let mut state = ParentReferenceState::default();
    let mut annotated = annotate_parent_expr(expr, &mut state)?;
    apply_slot_aliases(&mut annotated, &state);
    Ok(annotated)
}

fn annotate_parent_expr(
    expr: Value,
    state: &mut ParentReferenceState,
) -> Result<Value, ParserError> {
    let mut map = match expr {
        Value::Object(map) => map,
        other => return Ok(other),
    };

    let expr_type = map
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    if expr_type == "parent" {
        map.insert("slot".to_string(), new_parent_slot(state));
        annotate_field_array(&mut map, "stages", state)?;
        annotate_field_array(&mut map, "predicate", state)?;
        let slots = collect_push_from_children(
            &map,
            &[
                "type",
                "value",
                "position",
                "keepArray",
                "keepSingletonArray",
                "consarray",
                "tuple",
                "focus",
                "index",
                "slot",
                "ancestor",
                "seekingParent",
            ],
        );
        set_seeking_parent(&mut map, slots);
        return Ok(Value::Object(map));
    }

    if expr_type == "path" {
        return annotate_parent_path(map, state);
    }

    if expr_type == "function" || expr_type == "partial" {
        annotate_field_array(&mut map, "arguments", state)?;
        annotate_field_value(&mut map, "procedure", state)?;
        let slots = collect_push_from_children(
            &map,
            &[
                "procedure",
                "type",
                "value",
                "position",
                "name",
                "keepArray",
                "keepSingletonArray",
                "consarray",
                "tuple",
                "focus",
                "index",
                "slot",
                "ancestor",
                "seekingParent",
            ],
        );
        set_seeking_parent(&mut map, slots);
        return Ok(Value::Object(map));
    }

    if expr_type == "lambda" {
        annotate_field_value(&mut map, "body", state)?;
        set_seeking_parent(&mut map, Vec::new());
        return Ok(Value::Object(map));
    }

    if expr_type == "transform" {
        annotate_field_value(&mut map, "pattern", state)?;
        annotate_field_value(&mut map, "update", state)?;
        annotate_field_value(&mut map, "delete", state)?;
        set_seeking_parent(&mut map, Vec::new());
        return Ok(Value::Object(map));
    }

    if expr_type == "apply" {
        annotate_field_value(&mut map, "lhs", state)?;
        annotate_field_value(&mut map, "rhs", state)?;
        set_seeking_parent(&mut map, Vec::new());
        return Ok(Value::Object(map));
    }

    if expr_type == "bind" {
        annotate_field_value(&mut map, "lhs", state)?;
        annotate_field_value(&mut map, "rhs", state)?;
        let slots = collect_push_from_children(
            &map,
            &[
                "lhs",
                "type",
                "value",
                "position",
                "keepArray",
                "keepSingletonArray",
                "consarray",
                "tuple",
                "focus",
                "index",
                "slot",
                "ancestor",
                "seekingParent",
            ],
        );
        set_seeking_parent(&mut map, slots);
        return Ok(Value::Object(map));
    }

    if expr_type == "condition" {
        annotate_field_value(&mut map, "condition", state)?;
        annotate_field_value(&mut map, "then", state)?;
        annotate_field_value(&mut map, "else", state)?;
        let slots = collect_push_from_children(
            &map,
            &[
                "type",
                "value",
                "position",
                "keepArray",
                "keepSingletonArray",
                "consarray",
                "tuple",
                "focus",
                "index",
                "slot",
                "ancestor",
                "seekingParent",
            ],
        );
        set_seeking_parent(&mut map, slots);
        return Ok(Value::Object(map));
    }

    if expr_type == "block" {
        annotate_field_array(&mut map, "expressions", state)?;
        annotate_field_array(&mut map, "stages", state)?;
        annotate_field_array(&mut map, "predicate", state)?;
        let slots = collect_push_from_children(
            &map,
            &[
                "type",
                "value",
                "position",
                "keepArray",
                "keepSingletonArray",
                "consarray",
                "tuple",
                "focus",
                "index",
                "slot",
                "ancestor",
                "seekingParent",
            ],
        );
        set_seeking_parent(&mut map, slots);
        return Ok(Value::Object(map));
    }

    if expr_type == "unary" {
        let unary_value = map
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if unary_value == "{" {
            if let Some(lhs) = map.remove("lhs") {
                let mut pairs = Vec::new();
                for pair in lhs.as_array().cloned().unwrap_or_default() {
                    let pair_items = pair
                        .as_array()
                        .ok_or_else(|| ParserError::new("S0206", map_position(&map)))?;
                    if pair_items.len() != 2 {
                        return Err(ParserError::new("S0206", map_position(&map)));
                    }
                    let key = annotate_parent_expr(pair_items[0].clone(), state)?;
                    let value = annotate_parent_expr(pair_items[1].clone(), state)?;
                    pairs.push(Value::Array(vec![key, value]));
                }
                map.insert("lhs".to_string(), Value::Array(pairs));
            }
            annotate_field_array(&mut map, "stages", state)?;
            annotate_field_array(&mut map, "predicate", state)?;
            let slots = collect_push_from_children(
                &map,
                &[
                    "type",
                    "value",
                    "position",
                    "keepArray",
                    "keepSingletonArray",
                    "consarray",
                    "tuple",
                    "focus",
                    "index",
                    "slot",
                    "ancestor",
                    "seekingParent",
                ],
            );
            set_seeking_parent(&mut map, slots);
            return Ok(Value::Object(map));
        }
        annotate_field_value(&mut map, "expression", state)?;
        annotate_field_array(&mut map, "expressions", state)?;
        annotate_field_array(&mut map, "stages", state)?;
        annotate_field_array(&mut map, "predicate", state)?;
        let slots = collect_push_from_children(
            &map,
            &[
                "type",
                "value",
                "position",
                "keepArray",
                "keepSingletonArray",
                "consarray",
                "tuple",
                "focus",
                "index",
                "slot",
                "ancestor",
                "seekingParent",
            ],
        );
        set_seeking_parent(&mut map, slots);
        return Ok(Value::Object(map));
    }

    if expr_type == "binary" {
        annotate_field_value(&mut map, "lhs", state)?;
        annotate_field_value(&mut map, "rhs", state)?;
        annotate_field_array(&mut map, "stages", state)?;
        annotate_field_array(&mut map, "predicate", state)?;
        let slots = collect_push_from_children(
            &map,
            &[
                "type",
                "value",
                "position",
                "keepArray",
                "keepSingletonArray",
                "consarray",
                "tuple",
                "focus",
                "index",
                "slot",
                "ancestor",
                "seekingParent",
            ],
        );
        set_seeking_parent(&mut map, slots);
        return Ok(Value::Object(map));
    }

    if expr_type == "sort" {
        annotate_field_array(&mut map, "terms", state)?;
        annotate_field_array(&mut map, "stages", state)?;
        annotate_field_array(&mut map, "predicate", state)?;
        let slots = collect_push_from_children(
            &map,
            &[
                "type",
                "value",
                "position",
                "keepArray",
                "keepSingletonArray",
                "consarray",
                "tuple",
                "focus",
                "index",
                "slot",
                "ancestor",
                "seekingParent",
            ],
        );
        set_seeking_parent(&mut map, slots);
        return Ok(Value::Object(map));
    }

    annotate_field_value(&mut map, "expression", state)?;
    annotate_field_value(&mut map, "lhs", state)?;
    annotate_field_value(&mut map, "rhs", state)?;
    annotate_field_value(&mut map, "condition", state)?;
    annotate_field_value(&mut map, "then", state)?;
    annotate_field_value(&mut map, "else", state)?;
    annotate_field_value(&mut map, "procedure", state)?;
    annotate_field_value(&mut map, "body", state)?;
    annotate_field_value(&mut map, "pattern", state)?;
    annotate_field_value(&mut map, "update", state)?;
    annotate_field_value(&mut map, "delete", state)?;
    annotate_field_array(&mut map, "arguments", state)?;
    annotate_field_array(&mut map, "expressions", state)?;
    annotate_field_array(&mut map, "terms", state)?;
    annotate_field_array(&mut map, "steps", state)?;
    annotate_field_array(&mut map, "stages", state)?;
    annotate_field_array(&mut map, "predicate", state)?;
    annotate_field_value(&mut map, "group", state)?;
    annotate_field_value(&mut map, "expr", state)?;
    let slots = collect_push_from_children(
        &map,
        &[
            "type",
            "value",
            "position",
            "keepArray",
            "keepSingletonArray",
            "consarray",
            "tuple",
            "focus",
            "index",
            "slot",
            "ancestor",
            "seekingParent",
            "name",
            "descending",
        ],
    );
    set_seeking_parent(&mut map, slots);
    Ok(Value::Object(map))
}

fn annotate_parent_path(
    mut map: serde_json::Map<String, Value>,
    state: &mut ParentReferenceState,
) -> Result<Value, ParserError> {
    let position = map_position(&map);
    let mut steps = Vec::new();
    for step in map
        .remove("steps")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
    {
        steps.push(annotate_parent_expr(step, state)?);
    }

    let mut unresolved_slots = Vec::new();
    for step_index in 0..steps.len() {
        adjust_stage_slots(&mut steps[step_index], state)?;
        let step_slots = collect_push_slots(&steps[step_index]);
        for mut slot in step_slots {
            let mut search_index = step_index as isize - 1;
            while slot_level(&slot) > 0 {
                if search_index < 0 {
                    unresolved_slots.push(slot.clone());
                    break;
                }
                let mut candidate = search_index;
                while candidate > 0 {
                    let current_focus = has_focus(&steps[candidate as usize]);
                    let previous_focus = has_focus(&steps[(candidate - 1) as usize]);
                    if !(current_focus && previous_focus) {
                        break;
                    }
                    candidate -= 1;
                }
                let candidate_index = candidate as usize;
                let candidate_step = steps
                    .get_mut(candidate_index)
                    .ok_or_else(|| ParserError::new("S0206", position))?;
                seek_parent(candidate_step, &mut slot, state)?;
                search_index = candidate - 1;
            }
        }
    }

    map.insert("steps".to_string(), Value::Array(steps));
    set_seeking_parent(&mut map, unresolved_slots);
    Ok(Value::Object(map))
}

fn annotate_field_value(
    map: &mut serde_json::Map<String, Value>,
    key: &str,
    state: &mut ParentReferenceState,
) -> Result<(), ParserError> {
    let Some(value) = map.remove(key) else {
        return Ok(());
    };
    let analyzed = annotate_parent_expr(value, state)?;
    map.insert(key.to_string(), analyzed);
    Ok(())
}

fn annotate_field_array(
    map: &mut serde_json::Map<String, Value>,
    key: &str,
    state: &mut ParentReferenceState,
) -> Result<(), ParserError> {
    let Some(value) = map.remove(key) else {
        return Ok(());
    };
    if let Some(items) = value.as_array() {
        let mut analyzed = Vec::with_capacity(items.len());
        for item in items {
            analyzed.push(annotate_parent_expr(item.clone(), state)?);
        }
        map.insert(key.to_string(), Value::Array(analyzed));
        return Ok(());
    }
    let analyzed = annotate_parent_expr(value, state)?;
    map.insert(key.to_string(), analyzed);
    Ok(())
}

fn collect_push_slots(value: &Value) -> Vec<Value> {
    match value {
        Value::Array(items) => {
            let mut slots = Vec::new();
            for item in items {
                append_slots(&mut slots, collect_push_slots(item));
            }
            slots
        }
        Value::Object(map) => {
            let mut slots = Vec::new();
            if let Some(seeking_parent) = map.get("seekingParent").and_then(Value::as_array) {
                slots.extend(seeking_parent.clone());
            }
            if map
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|expr_type| expr_type == "parent")
            {
                if let Some(slot) = map.get("slot").cloned() {
                    slots.push(slot);
                }
            }
            slots
        }
        _ => Vec::new(),
    }
}

fn collect_push_from_children(
    map: &serde_json::Map<String, Value>,
    skip_keys: &[&str],
) -> Vec<Value> {
    let mut slots = Vec::new();
    for (key, value) in map {
        if skip_keys.iter().any(|skip| skip == key) {
            continue;
        }
        append_slots(&mut slots, collect_push_slots(value));
    }
    slots
}

fn append_slots(target: &mut Vec<Value>, mut slots: Vec<Value>) {
    target.append(&mut slots);
}

fn set_seeking_parent(map: &mut serde_json::Map<String, Value>, slots: Vec<Value>) {
    if slots.is_empty() {
        map.remove("seekingParent");
        return;
    }
    map.insert("seekingParent".to_string(), Value::Array(slots));
}

fn new_parent_slot(state: &mut ParentReferenceState) -> Value {
    let index = state.next_slot_index;
    state.next_slot_index += 1;
    json!({
        "label": format!("!{index}"),
        "level": 1,
        "index": index as u64,
    })
}

fn adjust_stage_slots(
    step: &mut Value,
    state: &mut ParentReferenceState,
) -> Result<(), ParserError> {
    let has_stage_fields = step
        .as_object()
        .is_some_and(|map| map.contains_key("stages") || map.contains_key("predicate"));
    if !has_stage_fields {
        return Ok(());
    }

    let mut slots = {
        let Some(step_map) = step.as_object_mut() else {
            return Ok(());
        };
        step_map
            .remove("seekingParent")
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default()
    };
    if slots.is_empty() {
        return Ok(());
    }

    let mut transformed = Vec::with_capacity(slots.len());
    for mut slot in slots.drain(..) {
        if slot_level(&slot) == 1 {
            seek_parent(step, &mut slot, state)?;
        } else {
            let level = slot_level(&slot) - 1;
            set_slot_level(&mut slot, level);
        }
        transformed.push(slot);
    }

    let Some(step_map) = step.as_object_mut() else {
        return Ok(());
    };
    set_seeking_parent(step_map, transformed);
    Ok(())
}

fn seek_parent(
    node: &mut Value,
    slot: &mut Value,
    state: &mut ParentReferenceState,
) -> Result<(), ParserError> {
    let node_position = expr_position(node);
    let node_type = node
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    if node_type == "name" || node_type == "wildcard" {
        let level = slot_level(slot) - 1;
        set_slot_level(slot, level);
        if level == 0 {
            let node_map = node
                .as_object_mut()
                .ok_or_else(|| ParserError::new("S0206", node_position))?;
            if let Some(existing_label) = node_map
                .get("ancestor")
                .and_then(|ancestor| ancestor.get("label"))
                .and_then(Value::as_str)
            {
                if let Some(slot_index) = slot_index(slot) {
                    state
                        .slot_label_aliases
                        .insert(slot_index, existing_label.to_string());
                }
                set_slot_label(slot, existing_label);
            }
            node_map.insert("ancestor".to_string(), slot.clone());
            node_map.insert("tuple".to_string(), Value::Bool(true));
        }
        return Ok(());
    }

    if node_type == "parent" {
        let level = slot_level(slot) + 1;
        set_slot_level(slot, level);
        return Ok(());
    }

    if node_type == "block" {
        let node_map = node
            .as_object_mut()
            .ok_or_else(|| ParserError::new("S0206", node_position))?;
        node_map.insert("tuple".to_string(), Value::Bool(true));
        if let Some(expressions) = node_map
            .get_mut("expressions")
            .and_then(Value::as_array_mut)
        {
            if let Some(last_expression) = expressions.last_mut() {
                seek_parent(last_expression, slot, state)?;
            }
        }
        return Ok(());
    }

    if node_type == "path" {
        let node_map = node
            .as_object_mut()
            .ok_or_else(|| ParserError::new("S0206", node_position))?;
        node_map.insert("tuple".to_string(), Value::Bool(true));
        let steps = node_map
            .get_mut("steps")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| ParserError::new("S0206", node_position))?;
        if steps.is_empty() {
            return Err(ParserError::new("S0217", node_position)
                .with_token(Value::String(node_type)));
        }
        let mut step_index = steps.len() as isize - 1;
        if let Some(last_step) = steps.get_mut(step_index as usize) {
            seek_parent(last_step, slot, state)?;
        }
        while slot_level(slot) > 0 && step_index > 0 {
            step_index -= 1;
            if let Some(step) = steps.get_mut(step_index as usize) {
                seek_parent(step, slot, state)?;
            }
        }
        return Ok(());
    }

    Err(
        ParserError::new("S0217", node_position)
            .with_token(Value::String(node_type)),
    )
}

fn has_focus(value: &Value) -> bool {
    value
        .as_object()
        .is_some_and(|map| map.contains_key("focus"))
}

fn slot_index(slot: &Value) -> Option<usize> {
    slot.get("index")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
}

fn slot_level(slot: &Value) -> i64 {
    slot.get("level")
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

fn set_slot_level(slot: &mut Value, level: i64) {
    if let Some(slot_map) = slot.as_object_mut() {
        slot_map.insert("level".to_string(), json!(level));
    }
}

fn set_slot_label(slot: &mut Value, label: &str) {
    if let Some(slot_map) = slot.as_object_mut() {
        slot_map.insert("label".to_string(), Value::String(label.to_string()));
    }
}

fn apply_slot_aliases(node: &mut Value, state: &ParentReferenceState) {
    match node {
        Value::Array(items) => {
            for item in items {
                apply_slot_aliases(item, state);
            }
        }
        Value::Object(map) => {
            if let Some(slot) = map.get_mut("slot") {
                apply_slot_alias(slot, state);
            }
            if let Some(ancestor) = map.get_mut("ancestor") {
                apply_slot_alias(ancestor, state);
            }
            if let Some(seeking_parent) = map
                .get_mut("seekingParent")
                .and_then(Value::as_array_mut)
            {
                for slot in seeking_parent {
                    apply_slot_alias(slot, state);
                }
            }
            for (key, value) in map.iter_mut() {
                if key == "slot" || key == "ancestor" || key == "seekingParent" {
                    continue;
                }
                apply_slot_aliases(value, state);
            }
        }
        _ => {}
    }
}

fn apply_slot_alias(slot: &mut Value, state: &ParentReferenceState) {
    let Some(index) = slot_index(slot) else {
        return;
    };
    let Some(label) = state.slot_label_aliases.get(&index) else {
        return;
    };
    set_slot_label(slot, label);
}

fn process_ast_object(mut map: serde_json::Map<String, Value>) -> Result<Value, ParserError> {
    let expr_type = map
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();

    match expr_type {
        "binary" => process_binary(Value::Object(map)),
        "unary" => process_unary(map),
        "function" | "partial" => process_function_or_partial(map),
        "lambda" => process_lambda(map),
        "condition" => process_condition(map),
        "transform" => process_transform(map),
        "block" => process_block(map),
        "path" => process_path_value(map),
        "name" => {
            let mut result = serde_json::Map::new();
            let step = Value::Object(map);
            let keep_singleton = step
                .get("keepArray")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            result.insert("type".to_string(), Value::String("path".to_string()));
            result.insert("steps".to_string(), Value::Array(vec![step]));
            if keep_singleton {
                result.insert("keepSingletonArray".to_string(), Value::Bool(true));
            }
            Ok(Value::Object(result))
        }
        "string" | "number" | "value" | "wildcard" | "descendant" | "variable" | "regex"
        | "parent" => Ok(Value::Object(map)),
        "operator" => process_operator(map),
        "error" => {
            if let Some(lhs) = map.remove("lhs") {
                return process_ast(lhs);
            }
            Ok(Value::Object(map))
        }
        _ => {
            let mut code = "S0206";
            if map
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| id == "(end)")
            {
                code = "S0207";
            }
            Err(
                ParserError::new(code, map_position(&map))
                    .with_token(map.get("value").cloned().unwrap_or(Value::Null)),
            )
        }
    }
}

fn process_binary(expr: Value) -> Result<Value, ParserError> {
    let op = expr
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_default();

    match op {
        "." => process_path(expr),
        "[" => process_predicate(expr),
        "{" => process_group_by(expr),
        "^" => process_order_by(expr),
        ":=" => process_bind(expr),
        "@" => process_focus_bind(expr),
        "#" => process_index_bind(expr),
        "~>" => process_apply(expr),
        _ => process_binary_default(expr),
    }
}

fn process_unary(mut map: serde_json::Map<String, Value>) -> Result<Value, ParserError> {
    let mut result = serde_json::Map::new();
    let value = map.get("value").cloned().unwrap_or(Value::Null);
    let position = map_position(&map);
    result.insert("type".to_string(), Value::String("unary".to_string()));
    result.insert("value".to_string(), value.clone());
    result.insert(
        "position".to_string(),
        Value::Number(serde_json::Number::from(position as u64)),
    );

    let unary_value = value.as_str().unwrap_or_default();
    if unary_value == "[" {
        let mut expressions = Vec::new();
        for item in map
            .remove("expressions")
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
        {
            expressions.push(process_ast(item)?);
        }
        result.insert("expressions".to_string(), Value::Array(expressions));
        return Ok(Value::Object(result));
    }

    if unary_value == "{" {
        let mut pairs = Vec::new();
        for pair in map
            .remove("lhs")
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
        {
            let pair_array = pair
                .as_array()
                .ok_or_else(|| ParserError::new("S0206", position))?;
            if pair_array.len() != 2 {
                return Err(ParserError::new("S0206", position));
            }
            pairs.push(Value::Array(vec![
                process_ast(pair_array[0].clone())?,
                process_ast(pair_array[1].clone())?,
            ]));
        }
        result.insert("lhs".to_string(), Value::Array(pairs));
        return Ok(Value::Object(result));
    }

    let expression = map
        .remove("expression")
        .ok_or_else(|| ParserError::new("S0206", position))?;
    let expression = process_ast(expression)?;
    if unary_value == "-" && is_type(&expression, "number") {
        let mut number = expression;
        if let Some(number_map) = number.as_object_mut() {
            if let Some(value) = number_map.get("value").and_then(Value::as_f64) {
                if let Some(number_value) = serde_json::Number::from_f64(-value) {
                    number_map.insert("value".to_string(), Value::Number(number_value));
                }
            }
        }
        return Ok(number);
    }
    result.insert("expression".to_string(), expression);
    Ok(Value::Object(result))
}

fn process_function_or_partial(
    mut map: serde_json::Map<String, Value>,
) -> Result<Value, ParserError> {
    let expr_type = map
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut result = serde_json::Map::new();
    result.insert("type".to_string(), Value::String(expr_type));
    if let Some(name) = map.get("name").cloned() {
        result.insert("name".to_string(), name);
    }
    if let Some(value) = map.get("value").cloned() {
        result.insert("value".to_string(), value);
    }
    result.insert(
        "position".to_string(),
        Value::Number(serde_json::Number::from(map_position(&map) as u64)),
    );

    let mut arguments = Vec::new();
    for argument in map
        .remove("arguments")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
    {
        arguments.push(process_ast(argument)?);
    }
    result.insert("arguments".to_string(), Value::Array(arguments));
    let procedure = map
        .remove("procedure")
        .ok_or_else(|| ParserError::new("S0206", map_position(&map)))?;
    result.insert("procedure".to_string(), process_ast(procedure)?);
    Ok(Value::Object(result))
}

fn process_lambda(mut map: serde_json::Map<String, Value>) -> Result<Value, ParserError> {
    let mut result = serde_json::Map::new();
    result.insert("type".to_string(), Value::String("lambda".to_string()));
    result.insert(
        "position".to_string(),
        Value::Number(serde_json::Number::from(map_position(&map) as u64)),
    );
    if let Some(arguments) = map.remove("arguments") {
        result.insert("arguments".to_string(), arguments);
    }
    if let Some(signature) = map.remove("signature") {
        result.insert("signature".to_string(), signature);
    }
    let body = map
        .remove("body")
        .ok_or_else(|| ParserError::new("S0206", map_position(&map)))?;
    result.insert("body".to_string(), process_ast(body)?);
    Ok(Value::Object(result))
}

fn process_condition(mut map: serde_json::Map<String, Value>) -> Result<Value, ParserError> {
    let mut result = serde_json::Map::new();
    result.insert("type".to_string(), Value::String("condition".to_string()));
    result.insert(
        "position".to_string(),
        Value::Number(serde_json::Number::from(map_position(&map) as u64)),
    );
    let condition = map
        .remove("condition")
        .ok_or_else(|| ParserError::new("S0206", map_position(&map)))?;
    let then_branch = map
        .remove("then")
        .ok_or_else(|| ParserError::new("S0206", map_position(&map)))?;
    result.insert("condition".to_string(), process_ast(condition)?);
    result.insert("then".to_string(), process_ast(then_branch)?);
    if let Some(else_branch) = map.remove("else") {
        result.insert("else".to_string(), process_ast(else_branch)?);
    }
    Ok(Value::Object(result))
}

fn process_transform(mut map: serde_json::Map<String, Value>) -> Result<Value, ParserError> {
    let mut result = serde_json::Map::new();
    result.insert("type".to_string(), Value::String("transform".to_string()));
    result.insert(
        "position".to_string(),
        Value::Number(serde_json::Number::from(map_position(&map) as u64)),
    );
    let pattern = map
        .remove("pattern")
        .ok_or_else(|| ParserError::new("S0206", map_position(&map)))?;
    let update = map
        .remove("update")
        .ok_or_else(|| ParserError::new("S0206", map_position(&map)))?;
    result.insert("pattern".to_string(), process_ast(pattern)?);
    result.insert("update".to_string(), process_ast(update)?);
    if let Some(delete_expr) = map.remove("delete") {
        result.insert("delete".to_string(), process_ast(delete_expr)?);
    }
    Ok(Value::Object(result))
}

fn process_block(mut map: serde_json::Map<String, Value>) -> Result<Value, ParserError> {
    let mut result = serde_json::Map::new();
    result.insert("type".to_string(), Value::String("block".to_string()));
    result.insert(
        "position".to_string(),
        Value::Number(serde_json::Number::from(map_position(&map) as u64)),
    );

    let mut expressions = Vec::new();
    let mut has_consarray = false;
    for item in map
        .remove("expressions")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
    {
        let part = process_ast(item)?;
        if part
            .get("consarray")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            has_consarray = true;
        }
        if is_type(&part, "path") {
            if let Some(first_step) = part.get("steps").and_then(Value::as_array).and_then(|steps| steps.first()) {
                if first_step
                    .get("consarray")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    has_consarray = true;
                }
            }
        }
        expressions.push(part);
    }

    result.insert("expressions".to_string(), Value::Array(expressions));
    if has_consarray {
        result.insert("consarray".to_string(), Value::Bool(true));
    }
    Ok(Value::Object(result))
}

fn process_path_value(mut map: serde_json::Map<String, Value>) -> Result<Value, ParserError> {
    let mut steps = Vec::new();
    for step in map
        .remove("steps")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
    {
        steps.push(process_ast(step)?);
    }
    map.insert("steps".to_string(), Value::Array(steps));
    Ok(Value::Object(map))
}

fn process_operator(mut map: serde_json::Map<String, Value>) -> Result<Value, ParserError> {
    let value = map.get("value").and_then(Value::as_str).unwrap_or_default();
    if value == "and" || value == "or" || value == "in" {
        map.insert("type".to_string(), Value::String("name".to_string()));
        return process_ast(Value::Object(map));
    }
    if value == "?" {
        return Ok(Value::Object(map));
    }
    Err(
        ParserError::new("S0201", map_position(&map))
            .with_token(map.get("value").cloned().unwrap_or(Value::Null)),
    )
}

fn process_path(expr: Value) -> Result<Value, ParserError> {
    let lhs = expr
        .get("lhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let rhs = expr
        .get("rhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;

    let processed_lhs = process_ast(lhs)?;
    let mut result = if is_type(&processed_lhs, "path") {
        processed_lhs
    } else {
        let mut path = serde_json::Map::new();
        path.insert("type".to_string(), Value::String("path".to_string()));
        path.insert("steps".to_string(), Value::Array(vec![processed_lhs]));
        Value::Object(path)
    };

    let mut processed_rhs = process_ast(rhs)?;
    if is_type(&processed_rhs, "function")
        && processed_rhs
            .get("procedure")
            .and_then(Value::as_object)
            .is_some_and(|procedure| {
                procedure
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|typ| typ == "path")
                    && procedure
                        .get("steps")
                        .and_then(Value::as_array)
                        .is_some_and(|steps| {
                            steps.len() == 1
                                && steps[0]
                                    .get("type")
                                    .and_then(Value::as_str)
                                    .is_some_and(|typ| typ == "name")
                        })
            })
    {
        if let Some(result_steps) = result
            .get_mut("steps")
            .and_then(Value::as_array_mut)
        {
            if let Some(last_step) = result_steps.last_mut() {
                if is_type(last_step, "function") {
                    if let Some(next_function) = processed_rhs
                        .get("procedure")
                        .and_then(Value::as_object)
                        .and_then(|procedure| procedure.get("steps"))
                        .and_then(Value::as_array)
                        .and_then(|steps| steps.first())
                        .and_then(|step| step.get("value"))
                        .cloned()
                    {
                        if let Some(last_step_map) = last_step.as_object_mut() {
                            last_step_map.insert("nextFunction".to_string(), next_function);
                        }
                    }
                }
            }
        }
    }

    if is_type(&processed_rhs, "path") {
        if let Some(rest_steps) = processed_rhs.get("steps").and_then(Value::as_array).cloned() {
            if let Some(result_steps) = result
                .get_mut("steps")
                .and_then(Value::as_array_mut)
            {
                result_steps.extend(rest_steps);
            }
        }
    } else {
        if processed_rhs.get("predicate").is_some() {
            if let Some(rhs_map) = processed_rhs.as_object_mut() {
                if let Some(predicate) = rhs_map.remove("predicate") {
                    rhs_map.insert("stages".to_string(), predicate);
                }
            }
        }
        if let Some(result_steps) = result
            .get_mut("steps")
            .and_then(Value::as_array_mut)
        {
            result_steps.push(processed_rhs);
        }
    }

    let steps = result
        .get_mut("steps")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    for step in steps.iter_mut() {
        let step_type = step
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if step_type == "number" || step_type == "value" {
            return Err(
                ParserError::new("S0213", step_position(step))
                    .with_value(step.get("value").cloned().unwrap_or(Value::Null)),
            );
        }
        if step_type == "string" {
            if let Some(step_map) = step.as_object_mut() {
                step_map.insert("type".to_string(), Value::String("name".to_string()));
            }
        }
    }

    if let Some(first_step) = steps.first_mut() {
        if is_type(first_step, "unary")
            && first_step
                .get("value")
                .and_then(Value::as_str)
                .is_some_and(|value| value == "[")
        {
            if let Some(step_map) = first_step.as_object_mut() {
                step_map.insert("consarray".to_string(), Value::Bool(true));
            }
        }
    }

    if let Some(last_step) = steps.last_mut() {
        if is_type(last_step, "unary")
            && last_step
                .get("value")
                .and_then(Value::as_str)
                .is_some_and(|value| value == "[")
        {
            if let Some(step_map) = last_step.as_object_mut() {
                step_map.insert("consarray".to_string(), Value::Bool(true));
            }
        }
    }

    if steps.iter().any(|step| {
        step.get("keepArray")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }) {
        if let Some(result_map) = result.as_object_mut() {
            result_map.insert("keepSingletonArray".to_string(), Value::Bool(true));
        }
    }

    Ok(result)
}

fn process_predicate(expr: Value) -> Result<Value, ParserError> {
    let lhs = expr
        .get("lhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let rhs = expr
        .get("rhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let position = expr_position(&expr);

    let mut result = process_ast(lhs)?;
    let predicate = process_ast(rhs)?;
    let filter = json!({
        "type": "filter",
        "expr": predicate,
        "position": position as u64,
    });

    if is_type(&result, "path") {
        let step = last_path_step_mut(&mut result, position)?;
        let step_map = step
            .as_object_mut()
            .ok_or_else(|| ParserError::new("S0206", position))?;
        if step_map.contains_key("group") {
            return Err(ParserError::new("S0209", position));
        }
        let stages = ensure_array_field(step_map, "stages", position)?;
        stages.push(filter);
        return Ok(result);
    }

    let step_map = result
        .as_object_mut()
        .ok_or_else(|| ParserError::new("S0206", position))?;
    if step_map.contains_key("group") {
        return Err(ParserError::new("S0209", position));
    }
    let predicates = ensure_array_field(step_map, "predicate", position)?;
    predicates.push(filter);
    Ok(result)
}

fn process_group_by(expr: Value) -> Result<Value, ParserError> {
    let lhs = expr
        .get("lhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let rhs = expr
        .get("rhs")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;

    let mut result = process_ast(lhs)?;
    let result_map = result
        .as_object_mut()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    if result_map.contains_key("group") {
        return Err(ParserError::new("S0210", expr_position(&expr)));
    }

    let mut group_pairs = Vec::with_capacity(rhs.len());
    for pair in rhs {
        let pair_array = pair
            .as_array()
            .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
        if pair_array.len() != 2 {
            return Err(ParserError::new("S0206", expr_position(&expr)));
        }
        group_pairs.push(Value::Array(vec![
            process_ast(pair_array[0].clone())?,
            process_ast(pair_array[1].clone())?,
        ]));
    }

    result_map.insert(
        "group".to_string(),
        json!({
            "lhs": group_pairs,
            "position": expr_position(&expr) as u64,
        }),
    );

    Ok(result)
}

fn process_order_by(expr: Value) -> Result<Value, ParserError> {
    let lhs = expr
        .get("lhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let rhs = expr
        .get("rhs")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;

    let mut result = process_ast(lhs)?;
    if !is_type(&result, "path") {
        let mut wrapped = serde_json::Map::new();
        wrapped.insert("type".to_string(), Value::String("path".to_string()));
        wrapped.insert("steps".to_string(), Value::Array(vec![result]));
        result = Value::Object(wrapped);
    }

    let mut terms = Vec::with_capacity(rhs.len());
    for term in rhs {
        let descending = term
            .get("descending")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let expression = term
            .get("expression")
            .cloned()
            .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
        terms.push(json!({
            "descending": descending,
            "expression": process_ast(expression)?,
        }));
    }

    if let Some(steps) = result.get_mut("steps").and_then(Value::as_array_mut) {
        steps.push(json!({
            "type": "sort",
            "position": expr_position(&expr) as u64,
            "terms": terms,
        }));
    }

    Ok(result)
}

fn process_bind(expr: Value) -> Result<Value, ParserError> {
    let lhs = expr
        .get("lhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let rhs = expr
        .get("rhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;

    Ok(json!({
        "type": "bind",
        "value": expr.get("value").cloned().unwrap_or(Value::Null),
        "position": expr_position(&expr) as u64,
        "lhs": process_ast(lhs)?,
        "rhs": process_ast(rhs)?,
    }))
}

fn process_focus_bind(expr: Value) -> Result<Value, ParserError> {
    let lhs = expr
        .get("lhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let rhs = expr
        .get("rhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;

    let mut result = process_ast(lhs)?;
    let step = if is_type(&result, "path") {
        last_path_step_mut(&mut result, expr_position(&expr))?
    } else {
        &mut result
    };

    let step_map = step
        .as_object_mut()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    if step_map.contains_key("stages") || step_map.contains_key("predicate") {
        return Err(ParserError::new("S0215", expr_position(&expr)));
    }
    if step_map
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|typ| typ == "sort")
    {
        return Err(ParserError::new("S0216", expr_position(&expr)));
    }
    if expr
        .get("keepArray")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        step_map.insert("keepArray".to_string(), Value::Bool(true));
    }
    step_map.insert(
        "focus".to_string(),
        rhs.get("value").cloned().unwrap_or(Value::Null),
    );
    step_map.insert("tuple".to_string(), Value::Bool(true));
    Ok(result)
}

fn process_index_bind(expr: Value) -> Result<Value, ParserError> {
    let lhs = expr
        .get("lhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let rhs = expr
        .get("rhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let index_value = rhs.get("value").cloned().unwrap_or(Value::Null);
    let position = expr_position(&expr);

    let mut result = process_ast(lhs)?;
    if !is_type(&result, "path") {
        if let Some(step_map) = result.as_object_mut() {
            if let Some(predicate) = step_map.remove("predicate") {
                step_map.insert("stages".to_string(), predicate);
            }
        }
        let mut wrapped = serde_json::Map::new();
        wrapped.insert("type".to_string(), Value::String("path".to_string()));
        wrapped.insert("steps".to_string(), Value::Array(vec![result]));
        result = Value::Object(wrapped);
    }

    let step = last_path_step_mut(&mut result, position)?;
    let step_map = step
        .as_object_mut()
        .ok_or_else(|| ParserError::new("S0206", position))?;
    if !step_map.contains_key("stages") {
        step_map.insert("index".to_string(), index_value);
    } else {
        let stages = ensure_array_field(step_map, "stages", position)?;
        stages.push(json!({
            "type": "index",
            "value": index_value,
            "position": position as u64,
        }));
    }
    step_map.insert("tuple".to_string(), Value::Bool(true));
    Ok(result)
}

fn process_apply(expr: Value) -> Result<Value, ParserError> {
    let lhs = expr
        .get("lhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let rhs = expr
        .get("rhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;

    let lhs = process_ast(lhs)?;
    let rhs = process_ast(rhs)?;
    let keep_array = lhs
        .get("keepArray")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || rhs
            .get("keepArray")
            .and_then(Value::as_bool)
            .unwrap_or(false);

    Ok(json!({
        "type": "apply",
        "value": expr.get("value").cloned().unwrap_or(Value::Null),
        "position": expr_position(&expr) as u64,
        "lhs": lhs,
        "rhs": rhs,
        "keepArray": keep_array,
    }))
}

fn process_binary_default(expr: Value) -> Result<Value, ParserError> {
    let lhs = expr
        .get("lhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;
    let rhs = expr
        .get("rhs")
        .cloned()
        .ok_or_else(|| ParserError::new("S0206", expr_position(&expr)))?;

    Ok(json!({
        "type": expr.get("type").cloned().unwrap_or(Value::Null),
        "value": expr.get("value").cloned().unwrap_or(Value::Null),
        "position": expr_position(&expr) as u64,
        "lhs": process_ast(lhs)?,
        "rhs": process_ast(rhs)?,
    }))
}

fn map_position(map: &serde_json::Map<String, Value>) -> usize {
    map.get("position")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize
}

fn is_type(value: &Value, expected: &str) -> bool {
    value
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|typ| typ == expected)
}

fn ensure_array_field<'a>(
    map: &'a mut serde_json::Map<String, Value>,
    name: &str,
    position: usize,
) -> Result<&'a mut Vec<Value>, ParserError> {
    let entry = map
        .entry(name.to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    entry
        .as_array_mut()
        .ok_or_else(|| ParserError::new("S0206", position))
}

fn last_path_step_mut(path: &mut Value, position: usize) -> Result<&mut Value, ParserError> {
    let path_map = path
        .as_object_mut()
        .ok_or_else(|| ParserError::new("S0206", position))?;
    let steps = path_map
        .get_mut("steps")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| ParserError::new("S0206", position))?;
    steps
        .last_mut()
        .ok_or_else(|| ParserError::new("S0206", position))
}

fn expr_position(expr: &Value) -> usize {
    expr.get("position")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize
}

fn step_position(step: &Value) -> usize {
    step.get("position")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize
}
