//! JSONata parser implementation translated from the upstream JavaScript Pratt parser.
//!
//! This module currently focuses on scaffolding the tokenizer and AST nodes so we can begin
//! porting the Pratt parser logic in a mostly mechanical fashion.

mod ast;
mod error;
mod parser;
mod tokenizer;

pub use ast::AstNode;
pub use error::ParserError;
pub use parser::Parser;

pub use tokenizer::{Token, TokenKind, Tokenizer};
