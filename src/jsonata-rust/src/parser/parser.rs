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

    pub fn parse(mut self) -> Result<AstNode, ParserError> {
        let expr = self.expression(0)?;
        if let Some(current) = &self.current {
            if current.id != "(end)" {
                return Err(ParserError::new("S0201", current.position)
                    .with_token(current.value.clone()));
            }
        }
        Ok(expr)
    }

    fn expression(&mut self, rbp: u32) -> Result<AstNode, ParserError> {
        let token = self
            .current
            .clone()
            .ok_or_else(|| ParserError::new("S0201", self.source.len()))?;
        self.advance(false)?;
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
                Ok(AstNode::new(
                    token.id,
                    token.token_type,
                    token.value,
                    token.position,
                ))
            }
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
            _ => Err(ParserError::new("S0211", token.position)
                .with_token(token.value.clone())),
        }
    }

    fn led(&mut self, token: TokenData, left: AstNode) -> Result<AstNode, ParserError> {
        match token.id.as_str() {
            "." => self.parse_dot(token, left),
            "[" => self.parse_filter(token, left),
            "(" => self.parse_function_call(token, left),
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
            | "|"
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
        let mut args = Vec::new();
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
        let mut node =
            AstNode::new(token.id, token.token_type, token.value, token.position);
        node.set_type("function");
        node.set_node("procedure", left);
        node.set_field(
            "arguments",
            Value::Array(args.into_iter().map(Into::into).collect()),
        );
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
        let mut node =
            AstNode::new(token.id, token.token_type, token.value, token.position);
        node.set_type("sort");
        node.set_node("lhs", left);
        node.set_field("terms", Value::Array(terms));
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
        let mut node =
            AstNode::new(token.id.clone(), token.token_type, token.value.clone(), token.position);
        node.set_type("condition");
        node.set_node("condition", left.clone());
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
        let mut node =
            AstNode::new(token.id, token.token_type, token.value, token.position);
        node.set_type("assignment");
        node.set_node("lhs", left);
        let rhs = self.expression(self.binding_power(":="))?;
        node.set_node("rhs", rhs);
        Ok(node)
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
    map.insert(";".to_string(), 80);
    map.insert(":".to_string(), 80);
    map.insert("?".to_string(), 20);
    map.insert("+".to_string(), 50);
    map.insert("-".to_string(), 50);
    map.insert("*".to_string(), 60);
    map.insert("/".to_string(), 60);
    map.insert("%".to_string(), 60);
    map.insert("|".to_string(), 20);
    map.insert("=".to_string(), 40);
    map.insert("<".to_string(), 40);
    map.insert(">".to_string(), 40);
    map.insert("^".to_string(), 40);
    map.insert("**".to_string(), 60);
    map.insert("..".to_string(), 20);
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
