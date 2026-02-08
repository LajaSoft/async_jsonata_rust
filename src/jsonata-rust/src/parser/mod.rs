//! JSONata parser implementation translated from the upstream JavaScript Pratt parser.
//!
//! This module currently focuses on scaffolding the tokenizer and AST nodes so we can begin
//! porting the Pratt parser logic in a mostly mechanical fashion.

mod ast;
mod error;
#[path = "parser-lib/mod.rs"]
mod parser_lib;
mod parser;
mod tokenizer;

use serde_json::Value;

pub use ast::AstNode;
pub use error::ParserError;
pub use parser::Parser;

pub use tokenizer::{Token, TokenKind, Tokenizer};

pub fn parse_expression(source: &str, recover: bool) -> Result<Value, ParserError> {
    Parser::new(source, recover)?.parse()
}
