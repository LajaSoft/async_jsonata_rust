use std::collections::HashMap;

use serde_json::Value;

use crate::error::Error;
use crate::evaluator;
use crate::parser;
use crate::registry;
use crate::types::{JsonFunction, JsonValue};

/// Parsed JSONata expression wrapper.
///
/// # Examples
/// ```rust
/// let expr = async_jsonata_rust::Parser::new().parse("Account.Order[0]")?;
/// assert_eq!(expr.source(), "Account.Order[0]");
/// # Ok::<(), async_jsonata_rust::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct Expression {
    source: String,
    ast: Value,
}

impl Expression {
    /// Returns original JSONata expression source.
    ///
    /// # Examples
    /// ```rust
    /// let expr = async_jsonata_rust::Parser::new().parse("1")?;
    /// assert_eq!(expr.source(), "1");
    /// # Ok::<(), async_jsonata_rust::Error>(())
    /// ```
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns parsed AST in JSON representation.
    ///
    /// # Examples
    /// ```rust
    /// let expr = async_jsonata_rust::Parser::new().parse("1")?;
    /// assert_eq!(expr.ast()["type"], "number");
    /// # Ok::<(), async_jsonata_rust::Error>(())
    /// ```
    pub fn ast(&self) -> &Value {
        &self.ast
    }

    /// Consumes wrapper and returns AST value.
    ///
    /// # Examples
    /// ```rust
    /// let expr = async_jsonata_rust::Parser::new().parse("1")?;
    /// let ast = expr.into_ast();
    /// assert!(ast.is_object());
    /// # Ok::<(), async_jsonata_rust::Error>(())
    /// ```
    pub fn into_ast(self) -> Value {
        self.ast
    }
}

/// Stable parser entry-point for parsing JSONata expressions.
///
/// # Examples
/// ```rust
/// let parser = async_jsonata_rust::Parser::new();
/// let expr = parser.parse("foo.bar")?;
/// assert_eq!(expr.ast()["type"], "path");
/// # Ok::<(), async_jsonata_rust::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct Parser {
    recover: bool,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    /// Creates parser with default strict mode (`recover = false`).
    ///
    /// # Examples
    /// ```rust
    /// let parser = async_jsonata_rust::Parser::new();
    /// let expr = parser.parse("a.b")?;
    /// assert_eq!(expr.ast()["type"], "path");
    /// # Ok::<(), async_jsonata_rust::Error>(())
    /// ```
    pub fn new() -> Self {
        Self { recover: false }
    }

    /// Enables/disables recover mode from JSONata parser.
    ///
    /// # Examples
    /// ```rust
    /// let parser = async_jsonata_rust::Parser::new().with_recover(true);
    /// let expr = parser.parse("1+")?;
    /// assert!(expr.ast().get("errors").is_some());
    /// # Ok::<(), async_jsonata_rust::Error>(())
    /// ```
    pub fn with_recover(mut self, recover: bool) -> Self {
        self.recover = recover;
        self
    }

    /// Parses expression and returns stable `Expression` wrapper.
    ///
    /// # Examples
    /// ```rust
    /// let expression = async_jsonata_rust::Parser::new().parse("foo")?;
    /// assert_eq!(expression.source(), "foo");
    /// # Ok::<(), async_jsonata_rust::Error>(())
    /// ```
    pub fn parse(&self, source: impl Into<String>) -> Result<Expression, Error> {
        let source = source.into();
        let ast = parser::parse_expression(&source, self.recover)?;
        Ok(Expression { source, ast })
    }
}

/// Stable function registry wrapper.
///
/// # Examples
/// ```rust
/// let registry = async_jsonata_rust::FunctionRegistry::with_builtins();
/// assert!(registry.contains("sqrt"));
/// ```
#[derive(Clone, Default)]
pub struct FunctionRegistry {
    functions: HashMap<String, JsonFunction>,
}

impl FunctionRegistry {
    /// Creates empty registry.
    ///
    /// # Examples
    /// ```rust
    /// let registry = async_jsonata_rust::FunctionRegistry::new();
    /// assert!(registry.is_empty());
    /// ```
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
        }
    }

    /// Creates registry preloaded with built-ins.
    ///
    /// # Examples
    /// ```rust
    /// let registry = async_jsonata_rust::FunctionRegistry::with_builtins();
    /// assert!(registry.contains("abs"));
    /// ```
    pub fn with_builtins() -> Self {
        Self {
            functions: registry::create_builtin_registry(),
        }
    }

    /// Registers function by name.
    ///
    /// # Examples
    /// ```rust
    /// use std::any::Any;
    /// use std::sync::Arc;
    /// use futures::future::BoxFuture;
    /// use async_jsonata_rust::types::{FunctionContext, JsonCallable, JsonError, JsonFunction, JsonValue};
    ///
    /// #[derive(Clone)]
    /// struct IdFn;
    /// impl JsonCallable for IdFn {
    ///     fn call(&self, _ctx: FunctionContext, args: Vec<JsonValue>) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
    ///         Box::pin(async move { Ok(args.into_iter().next().unwrap_or(JsonValue::Undefined)) })
    ///     }
    ///     fn as_any(&self) -> &(dyn Any + Send + Sync) { self }
    /// }
    ///
    /// let mut registry = async_jsonata_rust::FunctionRegistry::new();
    /// registry.insert("id", JsonFunction::new(Arc::new(IdFn)));
    /// assert!(registry.contains("id"));
    /// ```
    pub fn insert(&mut self, name: impl Into<String>, function: JsonFunction) {
        self.functions.insert(name.into(), function);
    }

    /// Gets function by name.
    ///
    /// # Examples
    /// ```rust
    /// let registry = async_jsonata_rust::FunctionRegistry::with_builtins();
    /// assert!(registry.get("sqrt").is_some());
    /// ```
    pub fn get(&self, name: &str) -> Option<&JsonFunction> {
        self.functions.get(name)
    }

    /// Checks if function exists.
    ///
    /// # Examples
    /// ```rust
    /// let registry = async_jsonata_rust::FunctionRegistry::with_builtins();
    /// assert!(registry.contains("round"));
    /// ```
    pub fn contains(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    /// Returns number of registered functions.
    ///
    /// # Examples
    /// ```rust
    /// let registry = async_jsonata_rust::FunctionRegistry::with_builtins();
    /// assert!(registry.len() > 0);
    /// ```
    pub fn len(&self) -> usize {
        self.functions.len()
    }

    /// Returns true for empty registry.
    ///
    /// # Examples
    /// ```rust
    /// let registry = async_jsonata_rust::FunctionRegistry::new();
    /// assert!(registry.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }

    /// Returns registry as map reference.
    ///
    /// # Examples
    /// ```rust
    /// let registry = async_jsonata_rust::FunctionRegistry::with_builtins();
    /// assert!(registry.as_map().contains_key("abs"));
    /// ```
    pub fn as_map(&self) -> &HashMap<String, JsonFunction> {
        &self.functions
    }

    /// Consumes wrapper and returns inner map.
    ///
    /// # Examples
    /// ```rust
    /// let registry = async_jsonata_rust::FunctionRegistry::with_builtins();
    /// let map = registry.into_inner();
    /// assert!(map.contains_key("abs"));
    /// ```
    pub fn into_inner(self) -> HashMap<String, JsonFunction> {
        self.functions
    }
}

/// Stable evaluator facade.
///
/// End-to-end evaluation is available for parser/runtime features currently implemented
/// in this crate. Full JSONata-js parity is still in progress.
///
/// # Examples
/// ```rust
/// let evaluator = async_jsonata_rust::Evaluator::with_builtins();
/// let expression = evaluator.parse("1")?;
/// let result = evaluator.evaluate(&expression, &async_jsonata_rust::JsonValue::Null);
/// assert_eq!(result?, async_jsonata_rust::JsonValue::Number(1.0));
/// # Ok::<(), async_jsonata_rust::Error>(())
/// ```
#[derive(Clone, Default)]
pub struct Evaluator {
    functions: FunctionRegistry,
}

impl Evaluator {
    /// Creates evaluator with empty registry.
    ///
    /// # Examples
    /// ```rust
    /// let evaluator = async_jsonata_rust::Evaluator::new(async_jsonata_rust::FunctionRegistry::new());
    /// assert!(evaluator.function_registry().is_empty());
    /// ```
    pub fn new(functions: FunctionRegistry) -> Self {
        Self { functions }
    }

    /// Creates evaluator with built-in registry.
    ///
    /// # Examples
    /// ```rust
    /// let evaluator = async_jsonata_rust::Evaluator::with_builtins();
    /// assert!(evaluator.function_registry().contains("sqrt"));
    /// ```
    pub fn with_builtins() -> Self {
        Self::new(FunctionRegistry::with_builtins())
    }

    /// Parses expression using stable parser API.
    ///
    /// # Examples
    /// ```rust
    /// let evaluator = async_jsonata_rust::Evaluator::with_builtins();
    /// let expression = evaluator.parse("foo.bar")?;
    /// assert_eq!(expression.ast()["type"], "path");
    /// # Ok::<(), async_jsonata_rust::Error>(())
    /// ```
    pub fn parse(&self, source: impl Into<String>) -> Result<Expression, Error> {
        Parser::new().parse(source)
    }

    /// Evaluates parsed expression against input JSON.
    ///
    /// # Examples
    /// ```rust
    /// let evaluator = async_jsonata_rust::Evaluator::with_builtins();
    /// let expression = evaluator.parse("1")?;
    /// let out = evaluator.evaluate(&expression, &async_jsonata_rust::JsonValue::Null);
    /// assert_eq!(out?, async_jsonata_rust::JsonValue::Number(1.0));
    /// # Ok::<(), async_jsonata_rust::Error>(())
    /// ```
    pub fn evaluate(&self, expression: &Expression, input: &JsonValue) -> Result<JsonValue, Error> {
        evaluator::evaluate_expression(expression.ast(), input, self.functions.as_map()).map_err(
            |err| err.with_context("expression", Value::String(expression.source().to_owned())),
        )
    }

    /// Evaluates parsed expression against input JSON with external variable bindings.
    ///
    /// Bindings are visible as variables (e.g. binding `"x"` is available as `$x` in expression).
    ///
    /// # Examples
    /// ```rust
    /// use std::collections::HashMap;
    /// use async_jsonata_rust::{Evaluator, JsonValue};
    ///
    /// let evaluator = Evaluator::with_builtins();
    /// let expression = evaluator.parse("$x + 1")?;
    /// let mut bindings = HashMap::new();
    /// bindings.insert("x".to_string(), JsonValue::Number(41.0));
    ///
    /// let out = evaluator.evaluate_with_bindings(&expression, &JsonValue::Null, &bindings)?;
    /// assert_eq!(out, JsonValue::Number(42.0));
    /// # Ok::<(), async_jsonata_rust::Error>(())
    /// ```
    pub fn evaluate_with_bindings(
        &self,
        expression: &Expression,
        input: &JsonValue,
        bindings: &HashMap<String, JsonValue>,
    ) -> Result<JsonValue, Error> {
        evaluator::evaluate_expression_with_bindings(
            expression.ast(),
            input,
            self.functions.as_map(),
            bindings,
        )
        .map_err(|err| {
            err.with_context("expression", Value::String(expression.source().to_owned()))
        })
    }

    /// Returns evaluator function registry.
    ///
    /// # Examples
    /// ```rust
    /// let evaluator = async_jsonata_rust::Evaluator::with_builtins();
    /// assert!(evaluator.function_registry().contains("abs"));
    /// ```
    pub fn function_registry(&self) -> &FunctionRegistry {
        &self.functions
    }
}
