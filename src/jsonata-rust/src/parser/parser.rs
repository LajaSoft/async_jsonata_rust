use std::collections::HashMap;

use serde_json::{json, Map, Value};

use super::ast::AstNode;
use super::error::ParserError;
use super::parser_lib::postprocess::{annotate_parent_references, expr_position, process_ast};
use super::parser_lib::signature::validate_signature_definition;
use super::parser_lib::syntax::{operator_table, token_value_to_json};
use super::tokenizer::{TokenKind, Tokenizer};

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
    recover: bool,
    pub errors: Vec<Value>,
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str, recover: bool) -> Result<Self, ParserError> {
        let mut parser = Self {
            source,
            tokenizer: Tokenizer::new(source),
            operators: operator_table(),
            current: None,
            recover,
            errors: Vec::new(),
        };
        parser.advance(false)?;
        Ok(parser)
    }

    pub fn parse(mut self) -> Result<Value, ParserError> {
        let expr = if self.recover {
            match self.expression(0) {
                Ok(expr) => expr,
                Err(err) => self.error_node_wrapped(err, true, false),
            }
        } else {
            self.expression(0)?
        };
        if let Some(current) = &self.current {
            if current.id != "(end)" {
                let err = ParserError::new("S0201", current.position)
                    .with_token(current.value.clone());
                if self.recover {
                    self.push_error(err, true, None, false);
                } else {
                    return Err(err);
                }
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
        let mut processed = processed;
        if self.recover && !self.errors.is_empty() {
            self.sync_errors_from_ast(&processed);
            let mut delayed_s0207 = Vec::new();
            let mut ordered = Vec::new();
            for err in self.errors.drain(..) {
                let should_delay = err
                    .get("code")
                    .and_then(Value::as_str)
                    .is_some_and(|code| code == "S0207")
                    && err
                        .get("token")
                        .and_then(Value::as_str)
                        .is_some_and(|token| token == "(end)")
                    && err.get("remaining").is_none();
                if should_delay {
                    delayed_s0207.push(err);
                } else {
                    ordered.push(err);
                }
            }
            ordered.extend(delayed_s0207);
            self.errors = ordered;
            if let Some(map) = processed.as_object_mut() {
                map.insert("errors".to_string(), Value::Array(self.errors.clone()));
            }
        }
        Ok(processed)
    }

    fn expression(&mut self, rbp: u32) -> Result<AstNode, ParserError> {
        let token = self
            .current
            .clone()
            .ok_or_else(|| ParserError::new("S0201", self.source.len()))?;
        let token_is_end = token.id == "(end)";
        self.advance(true)?;
        let mut left = match self.nud(token) {
            Ok(node) => node,
            Err(err) => {
                if self.recover {
                    if token_is_end {
                        self.error_node_wrapped(err, false, false)
                    } else {
                        self.error_node_inline(err, true, false)
                    }
                } else {
                    return Err(err);
                }
            }
        };
        while let Some(current) = &self.current {
            let lbp = self.binding_power(&current.id);
            if rbp >= lbp {
                break;
            }
            let token = current.clone();
            self.advance(true)?;
            left = match self.led(token, left.clone()) {
                Ok(node) => node,
                Err(err) => {
                    if self.recover {
                        let mut node = self.error_node_inline(err, false, false);
                        node.set_node("lhs", left);
                        node
                    } else {
                        return Err(err);
                    }
                }
            };
        }
        Ok(left)
    }

    fn nud(&mut self, token: TokenData) -> Result<AstNode, ParserError> {
        if token.token_type == "operator"
            && matches!(token.id.as_str(), "and" | "or" | "in" | "?")
        {
            return Ok(AstNode::new(
                token.id,
                token.token_type,
                token.value,
                token.position,
            ));
        }

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
            "@" => self.parse_focus_bind(token, left),
            "#" => self.parse_index_bind(token, left),
            ":" => Err(ParserError::new("S0201", token.position)),
            "+" | "-" | "*" | "/" | "%" | "=" | "<" | ">" | "!=" | "<=" | ">=" | "&" | "and"
            | "or" | "in" | "~>" => self.parse_binary(token, left),
            "??" => self.parse_coalesce(token, left),
            "?" => self.parse_ternary(token, left),
            "?:" => self.parse_default(token, left),
            ":=" => self.parse_assignment(token, left),
            ".."
            | ";" => self.parse_binary(token, left),
            _ => Err(ParserError::new("S0201", token.position)),
        }
    }

    fn binding_power(&self, id: &str) -> u32 {
        self.operators.get(id).copied().unwrap_or(0)
    }

    fn mark_keep_array_in_value(value: &mut Value) {
        let Some(map) = value.as_object_mut() else {
            return;
        };

        let node_type = map.get("type").and_then(Value::as_str).unwrap_or_default();
        let node_value = map.get("value").and_then(Value::as_str).unwrap_or_default();
        if node_type == "binary" && node_value == "[" {
            if let Some(lhs) = map.get_mut("lhs") {
                Self::mark_keep_array_in_value(lhs);
                return;
            }
        }

        map.insert("keepArray".to_string(), Value::Bool(true));
    }

    fn mark_keep_array_target(node: &mut AstNode) {
        if node.node_type == "binary" && node.value.as_str().is_some_and(|value| value == "[") {
            if let Some(lhs) = node.fields.get_mut("lhs") {
                Self::mark_keep_array_in_value(lhs);
                return;
            }
        }
        node.set_field("keepArray", Value::Bool(true));
    }

    fn token_data_to_json(token: &TokenData) -> Value {
        json!({
            "type": token.token_type,
            "value": token.value,
            "position": token.position
        })
    }

    fn parser_error_to_value(err: &ParserError) -> Value {
        let mut map = Map::new();
        map.insert("code".to_string(), Value::String(err.code.clone()));
        map.insert("position".to_string(), json!(err.position as u64));
        if let Some(token) = &err.token {
            map.insert("token".to_string(), token.clone());
        }
        if let Some(value) = &err.value {
            map.insert("value".to_string(), value.clone());
        }
        if let Some(remaining) = &err.remaining {
            map.insert("remaining".to_string(), Value::Array(remaining.clone()));
        }
        Value::Object(map)
    }

    fn collect_inline_errors(node: &Value, out: &mut Vec<Value>) {
        match node {
            Value::Object(map) => {
                if map
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| kind == "error")
                    && map.contains_key("code")
                {
                    out.push(Value::Object(map.clone()));
                }
                for value in map.values() {
                    Self::collect_inline_errors(value, out);
                }
            }
            Value::Array(items) => {
                for item in items {
                    Self::collect_inline_errors(item, out);
                }
            }
            _ => {}
        }
    }

    fn sync_errors_from_ast(&mut self, ast: &Value) {
        let mut inline_errors = Vec::new();
        Self::collect_inline_errors(ast, &mut inline_errors);
        for inline in inline_errors {
            let Some(inline_obj) = inline.as_object() else {
                continue;
            };
            let code = inline_obj.get("code").and_then(Value::as_str);
            let position = inline_obj.get("position").and_then(Value::as_u64);
            let token = inline_obj.get("token");

            for existing in &mut self.errors {
                let Some(existing_obj) = existing.as_object() else {
                    continue;
                };
                let code_matches = existing_obj
                    .get("code")
                    .and_then(Value::as_str)
                    == code;
                let position_matches = existing_obj
                    .get("position")
                    .and_then(Value::as_u64)
                    == position;
                let token_matches = existing_obj.get("token") == token;
                if code_matches && position_matches && token_matches {
                    *existing = inline.clone();
                    break;
                }
            }
        }
    }

    fn remaining_tokens(&mut self) -> Vec<Value> {
        let mut remaining = Vec::new();
        if let Some(current) = &self.current {
            if current.id != "(end)" {
                remaining.push(Self::token_data_to_json(current));
            }
        }
        while let Ok(Some(token)) = self.tokenizer.next(false) {
            remaining.push(json!({
                "type": match token.kind {
                    TokenKind::Operator => "operator",
                    TokenKind::Number => "number",
                    TokenKind::String => "string",
                    TokenKind::Regex => "regex",
                    TokenKind::Name => "name",
                    TokenKind::Variable => "variable",
                    TokenKind::Value => "value",
                    TokenKind::Eof => "end",
                },
                "value": token_value_to_json(&token.value),
                "position": token.position
            }));
        }
        remaining
    }

    fn consume_to_end(&mut self) {
        while matches!(self.current.as_ref().map(|token| token.id.as_str()), Some(id) if id != "(end)") {
            if self.advance(false).is_err() {
                break;
            }
        }
    }

    fn push_error(
        &mut self,
        mut err: ParserError,
        attach_remaining: bool,
        error_type: Option<&str>,
        consume_rest: bool,
    ) -> Value {
        if attach_remaining && err.remaining.is_none() {
            err = err.with_remaining(self.remaining_tokens());
        }
        let mut value = Self::parser_error_to_value(&err);
        if let Some(error_type) = error_type {
            if let Some(map) = value.as_object_mut() {
                map.insert("type".to_string(), Value::String(error_type.to_string()));
            }
        }
        self.errors.push(value.clone());
        if consume_rest {
            self.consume_to_end();
        }
        value
    }

    fn error_node_inline(
        &mut self,
        err: ParserError,
        attach_remaining: bool,
        consume_rest: bool,
    ) -> AstNode {
        let node_position = err.position;
        let err_value = self.push_error(err, attach_remaining, Some("error"), consume_rest);
        let mut node = AstNode::new(
            "(error)".to_string(),
            "error".to_string(),
            Value::Null,
            node_position,
        );
        node.set_type("error");
        if let Some(err_map) = err_value.as_object() {
            for (key, value) in err_map {
                if key == "type" {
                    continue;
                }
                node.set_field(key, value.clone());
            }
        }
        node
    }

    fn error_node_wrapped(
        &mut self,
        err: ParserError,
        attach_remaining: bool,
        consume_rest: bool,
    ) -> AstNode {
        let node_position = err.position;
        let err_value = self.push_error(err, attach_remaining, None, consume_rest);
        let mut node = AstNode::new(
            "(error)".to_string(),
            "error".to_string(),
            Value::Null,
            node_position,
        );
        node.set_type("error");
        node.set_field("error", err_value);
        node
    }

    fn advance(&mut self, infix: bool) -> Result<(), ParserError> {
        let token = match self.next_token(infix)? {
            Some(token) => token,
            None => TokenData {
                id: "(end)".to_string(),
                token_type: "end".to_string(),
                value: Value::String("(end)".to_string()),
                position: self.source.len(),
            },
        };
        if token.token_type == "operator" && !is_known_operator(&token.id) {
            return Err(
                ParserError::new("S0204", token.position)
                    .with_token(Value::String(token.id.clone())),
            );
        }
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
                Self::mark_keep_array_target(&mut left);
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
        let mut has_partial_argument = false;
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
                has_partial_argument = true;
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
        if has_partial_argument {
            node.set_type("partial");
        } else {
            node.set_type("function");
        }
        node.set_node("procedure", left);
        node.set_field("arguments", arguments_value);
        Ok(node)
    }

    fn parse_focus_bind(&mut self, token: TokenData, left: AstNode) -> Result<AstNode, ParserError> {
        let mut node =
            AstNode::new(token.id.clone(), "operator".to_string(), Value::String(token.id), token.position);
        node.set_type("binary");
        node.set_node("lhs", left);
        let rhs = self.expression(self.binding_power("@"))?;
        if rhs.node_type != "variable" {
            return Err(ParserError::new("S0214", rhs.position).with_token(Value::String("@".to_string())));
        }
        node.set_node("rhs", rhs);
        Ok(node)
    }

    fn parse_index_bind(&mut self, token: TokenData, left: AstNode) -> Result<AstNode, ParserError> {
        let mut node =
            AstNode::new(token.id.clone(), "operator".to_string(), Value::String(token.id), token.position);
        node.set_type("binary");
        node.set_node("lhs", left);
        let rhs = self.expression(self.binding_power("#"))?;
        if rhs.node_type != "variable" {
            return Err(ParserError::new("S0214", rhs.position).with_token(Value::String("#".to_string())));
        }
        node.set_node("rhs", rhs);
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
                let code = if current.id == "(end)" { "S0203" } else { "S0202" };
                let err = ParserError::new(code, current.position)
                    .with_token(Value::String(current.id.clone()))
                    .with_value(Value::String(id.to_string()));
                if self.recover {
                    self.push_error(err, true, None, true);
                    return Ok(());
                }
                return Err(err);
            }
        } else {
            let err = ParserError::new("S0203", self.source.len());
            if self.recover {
                self.push_error(err, true, None, true);
                return Ok(());
            }
            return Err(err);
        }
        self.advance(infix)
    }
}

fn is_lambda_name(node: &AstNode) -> bool {
    node.node_type == "name" && (node.id == "function" || node.id == "\u{03BB}")
}

fn is_known_operator(id: &str) -> bool {
    matches!(
        id,
        "."
            | "["
            | "]"
            | "{"
            | "}"
            | "("
            | ")"
            | ","
            | "@"
            | "#"
            | ";"
            | ":"
            | "+"
            | "-"
            | "*"
            | "**"
            | "/"
            | "%"
            | "<"
            | ">"
            | "="
            | "!="
            | "<="
            | ">="
            | "&"
            | "|"
            | "^"
            | "~>"
            | "?"
            | "??"
            | "?:"
            | ":="
            | ".."
            | "and"
            | "or"
            | "in"
    )
}
