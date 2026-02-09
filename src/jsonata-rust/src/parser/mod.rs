//! Low-level JSONata parser module.
//!
//! Prefer crate-level [`crate::Parser`] for stable public API.

mod ast;
mod error;
mod parser;
#[path = "parser-lib/mod.rs"]
mod parser_lib;
mod tokenizer;

use serde_json::Value;

pub use ast::AstNode;
pub use error::ParserError;
pub use parser::Parser;

pub use tokenizer::{Token, TokenKind, Tokenizer};

/// Parses JSONata expression into AST JSON representation.
///
/// # Examples
/// ```rust
/// let ast = async_jsonata_rust::parser::parse_expression("Account.Order[0]", false)?;
/// assert!(ast.is_object());
/// # Ok::<(), async_jsonata_rust::parser::ParserError>(())
/// ```
pub fn parse_expression(source: &str, recover: bool) -> Result<Value, ParserError> {
    Parser::new(source, recover)?.parse()
}
