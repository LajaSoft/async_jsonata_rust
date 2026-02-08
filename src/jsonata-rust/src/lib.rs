//! `jsonata_rust` is an async-first Rust implementation of JSONata.
//!
//! This crate currently exposes parser APIs, core JSONata value/function types,
//! builtin function registry wiring, and async-capable function execution helpers.
//!
//! JSONata language reference:
//! - <https://docs.jsonata.org/overview>
//! - <https://docs.jsonata.org/path-operators>
//! - <https://docs.jsonata.org/programming>
//!
//! # Quick Start
//! ```rust
//! use jsonata_rust::parser;
//!
//! let ast = parser::parse_expression("Account.Order[0].Product", false)
//!     .expect("expression should parse");
//! assert!(ast.is_object());
//! ```
//!
//! # Async Callable Example
//! ```rust
//! use std::any::Any;
//! use std::sync::Arc;
//!
//! use futures::executor::block_on;
//! use futures::future::BoxFuture;
//! use jsonata_rust::functions::core;
//! use jsonata_rust::types::{FunctionContext, JsonArray, JsonCallable, JsonError, JsonFunction, JsonValue};
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
//!     fn arity(&self) -> Option<usize> { Some(1) }
//!
//!     fn as_any(&self) -> &(dyn Any + Send + Sync) { self }
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

pub mod functions;
pub mod parser;
pub mod registry;
pub mod types;

pub use parser::{parse_expression, Parser, ParserError};
pub use registry::create_builtin_registry;
pub use types::{
    JsonataArray, JsonataCallable, JsonataFunction, JsonataObject, JsonataValue, NativeRef,
    NativeType,
};
