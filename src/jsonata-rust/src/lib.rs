#![allow(clippy::approx_constant)]
#![allow(clippy::cast_abs_to_unsigned)]
#![allow(clippy::get_first)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::module_inception)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::result_large_err)]
#![allow(clippy::type_complexity)]
#![allow(clippy::unnecessary_map_or)]

//! Async-first JSONata crate for Rust.
//!
//! This crate exposes a stable public surface focused on:
//! - JSONata parsing (`Parser`, `Expression`)
//! - function registry management (`FunctionRegistry`)
//! - async custom functions (`types::JsonCallable` / `types::JsonataCallable`)
//! - evaluator facade (`Evaluator`, runtime parity in progress)
//!
//! JSONata language reference:
//! - <https://docs.jsonata.org/overview>
//! - <https://docs.jsonata.org/path-operators>
//! - <https://docs.jsonata.org/programming>
//!
//! # Quick Start
//! ```rust
//! use async_jsonata_rust::Parser;
//!
//! let parser = Parser::new();
//! let expression = parser.parse("Account.Order[0].Product")?;
//! assert_eq!(expression.ast()["type"], "path");
//! # Ok::<(), async_jsonata_rust::Error>(())
//! ```
//!
//! # JSONata syntax
//! `async_jsonata_rust` follows JSONata syntax from the official docs and upstream test-suite.
//! Parser support is production-oriented, while evaluator parity is declared separately in
//! `docs/compatibility.md` and crate README.
//!
//! # Async functions
//! ```rust
//! use std::any::Any;
//! use std::sync::Arc;
//!
//! use futures::executor::block_on;
//! use futures::future::BoxFuture;
//! use async_jsonata_rust::functions::core;
//! use async_jsonata_rust::types::{FunctionContext, JsonArray, JsonCallable, JsonError, JsonFunction, JsonValue};
//!
//! #[derive(Clone)]
//! struct DoubleCallable;
//!
//! impl JsonCallable for DoubleCallable {
//!     fn call(&self, _ctx: FunctionContext, args: Vec<JsonValue>) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
//!         let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
//!         Box::pin(async move {
//!             if let JsonValue::Number(value) = input {
//!                 return Ok(JsonValue::Number(value * 2.0));
//!             }
//!             Ok(JsonValue::Undefined)
//!         })
//!     }
//!
//!     fn arity(&self) -> Option<usize> {
//!         Some(1)
//!     }
//!
//!     fn as_any(&self) -> &(dyn Any + Send + Sync) {
//!         self
//!     }
//! }
//!
//! let input = JsonValue::Array(JsonArray::new(
//!     vec![JsonValue::Number(1.0), JsonValue::Number(2.0)],
//!     true,
//!     false,
//! ));
//! let func = JsonValue::Function(JsonFunction::new(Arc::new(DoubleCallable)));
//! let out = block_on(core::map(FunctionContext::empty(), input, func)).unwrap();
//! assert!(matches!(out, JsonValue::Array(_)));
//! ```
//!
//! # Errors
//! Stable APIs return unified [`Error`] with JSONata-style code (`S0201`, `D3040`, ...)
//! and structured context fields.

pub mod api;
pub mod error;
mod evaluator;
pub mod functions;
pub mod parser;
pub mod registry;
pub mod types;

pub use api::{
    Evaluator, EvaluatorOptions, Expression, FunctionRegistry, Parser,
    DEFAULT_MAX_NON_TAIL_DEPTH, DEFAULT_MAX_TAIL_CALL_STEPS, DEFAULT_STACK_GROW_SIZE,
    DEFAULT_STACK_RED_ZONE, DEFAULT_SYNC_STACK_SIZE,
};
pub use error::Error;

pub use parser::{
    parse_expression, AstNode, Parser as LowLevelParser, ParserError, Token, TokenKind, Tokenizer,
};
pub use registry::{create_builtin_registry, lookup_builtin};
pub use types::{
    JsonArray, JsonCallable, JsonError, JsonFunction, JsonObject, JsonValue, JsonataArray,
    JsonataCallable, JsonataFocus, JsonataFunction, JsonataObject, JsonataValue, NativeRef,
    NativeType,
};
