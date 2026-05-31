//! A standalone application that uses the published `async_jsonata_rust` crate
//! (pulled from crates.io) to turn a raw e-commerce JSON document into a summary
//! report with a single complex JSONata expression, evaluated through the
//! pure-Rust ASYNC engine.
//!
//! It demonstrates, in one expression:
//!   - path navigation, predicates and projections (`orders`, `[gross >= 100]`, `.{...}`)
//!   - aggregation built-ins (`$sum`, `$average`, `$count`, `$round`, `$distinct`)
//!   - string built-ins (`$uppercase`)
//!   - the sort operator (`^(>price)`) and indexing
//!   - higher-order functions and lambdas (`~> $map(function($o){...})`)
//!   - block scope and local variables (`$orderTotal := ...`)
//!   - a CUSTOM, user-registered ASYNC function (`$discount`) that the engine
//!     awaits cooperatively (it yields `Poll::Pending` once, as a stand-in for
//!     an async rate lookup) — proving custom async functions compose with the
//!     async evaluator.

use std::any::Any;
use std::sync::Arc;
use std::task::Poll;

use async_jsonata_rust::types::{FunctionContext, JsonCallable, JsonError, JsonFunction, JsonValue};
use async_jsonata_rust::{Evaluator, FunctionRegistry};
use futures::executor::block_on;
use futures::future::BoxFuture;
use serde_json::json;

/// A user-defined async built-in: `$discount(amount, region)`.
///
/// Returns `amount` reduced by a region-specific rate. The future deliberately
/// yields `Poll::Pending` once before resolving, simulating an awaited rate
/// lookup (a DB/HTTP call in a real app). The async evaluator drives it to
/// completion via `.await` — no blocking, no threads.
#[derive(Clone)]
struct DiscountFn;

impl JsonCallable for DiscountFn {
    fn call(
        &self,
        _ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
        let amount = match args.first() {
            Some(JsonValue::Number(value)) => *value,
            _ => return Box::pin(async { Ok(JsonValue::Undefined) }),
        };
        let region = match args.get(1) {
            Some(JsonValue::String(text)) => text.clone(),
            _ => String::new(),
        };

        let mut yielded = false;
        Box::pin(futures::future::poll_fn(move |cx| {
            if !yielded {
                // Pretend we are awaiting an async rate service: yield control
                // back to the executor once, then resume.
                yielded = true;
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            let rate = match region.as_str() {
                "EU" => 0.10,
                "US" => 0.05,
                _ => 0.0,
            };
            Poll::Ready(Ok(JsonValue::Number(amount * (1.0 - rate))))
        }))
    }

    fn arity(&self) -> Option<usize> {
        Some(2)
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}

const REPORT_EXPR: &str = r#"
(
  $orderTotal := function($o){ $sum($o.items.(price * qty)) };

  $report := orders ~> $map(function($o){
    {
      "id": $o.id,
      "customer": $uppercase($o.customer),
      "region": $o.region,
      "units": $sum($o.items.qty),
      "gross": $round($orderTotal($o), 2),
      "net": $round($discount($orderTotal($o), $o.region), 2),
      "topItem": ($o.items^(>price))[0].name
    }
  });

  {
    "orderCount": $count(orders),
    "regions": $distinct($report.region),
    "byRevenueDesc": ($report^(>gross)).id,
    "grandGross": $round($sum($report.gross), 2),
    "grandNet": $round($sum($report.net), 2),
    "avgUnitsPerOrder": $round($average($report.units), 1),
    "bigOrders": $report[gross >= 100].{ "id": id, "gross": gross },
    "report": $report
  }
)
"#;

fn main() {
    let input_json = json!({
      "orders": [
        {
          "id": "A-1001",
          "customer": "alice",
          "region": "EU",
          "items": [
            {"sku": "kbd-01", "name": "Keyboard",  "price": 45.00, "qty": 2},
            {"sku": "mse-07", "name": "Mouse",     "price": 19.50, "qty": 3}
          ]
        },
        {
          "id": "A-1002",
          "customer": "bob",
          "region": "US",
          "items": [
            {"sku": "mon-27", "name": "Monitor 27\"", "price": 240.00, "qty": 1},
            {"sku": "cbl-hd", "name": "HDMI Cable",   "price": 8.00,  "qty": 4}
          ]
        },
        {
          "id": "A-1003",
          "customer": "carol",
          "region": "EU",
          "items": [
            {"sku": "usb-c",  "name": "USB-C Hub",  "price": 60.00, "qty": 1},
            {"sku": "pad-01", "name": "Mouse Pad",  "price": 12.00, "qty": 5}
          ]
        }
      ]
    });

    // Builtins + our custom async function, all in one registry.
    let mut registry = FunctionRegistry::with_builtins();
    registry.insert("discount", JsonFunction::new(Arc::new(DiscountFn)));
    let evaluator = Evaluator::new(registry);

    let expression = evaluator
        .parse(REPORT_EXPR)
        .expect("report expression should parse");
    let input = JsonValue::from_serde_json(&input_json);

    println!("== Input JSON ==");
    println!("{}\n", serde_json::to_string_pretty(&input_json).unwrap());
    println!("== JSONata expression ==");
    println!("{}\n", REPORT_EXPR.trim());

    // Drive the genuinely-async evaluator on a futures executor. The custom
    // `$discount` function suspends (Poll::Pending) and is awaited cooperatively
    // — exactly how it would behave under tokio/async-std.
    let result = block_on(evaluator.evaluate_async(&expression, &input))
        .expect("report expression should evaluate");

    let output = result
        .to_serde_json()
        .expect("report result should be representable as JSON");

    println!("== Report (async evaluation, crate from crates.io) ==");
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}
