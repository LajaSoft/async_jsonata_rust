use std::any::Any;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::Poll;

use futures::executor::block_on;
use futures::future::BoxFuture;
use async_jsonata_rust::functions::{core, math, strings};
use async_jsonata_rust::parser;
use async_jsonata_rust::types::{
    FunctionContext, JsonArray, JsonCallable, JsonError, JsonFunction, JsonValue, JsonataArray,
    JsonataValue,
};
use serde_json::json;

#[derive(Clone)]
struct DoubleCallable;
#[derive(Clone)]
struct YieldOnceDoubleCallable {
    pending_polls: Arc<AtomicUsize>,
}

const COMPLEX_ALL_IN_EXPR: &str = r#"
(
  $norm := function($o){(
    $lineTotal := $sum($o.items.(price * qty));
    {
      "id": $o.id,
      "customer": $uppercase($o.customer),
      "lineTotal": $round($lineTotal, 2),
      "positiveItemCount": $count($o.items[qty > 0]),
      "firstSku": $o.items[0].sku
    }
  )};

  $selected := orders[testkey = 1 and $count(items[qty > 0]) > 0];
  $mapped := $selected ~> $map($norm);
  $regionObj := $sift(accounts, function($v, $k){$contains($k, "eu")});

  {
    "selectedCount": $count($selected),
    "idsViaChain": $selected ~> $map(function($o){$o.id}),
    "mapped": $mapped,
    "sortedIdsByTotalDesc": ($mapped^(>lineTotal)).id,
    "grandTotal": $mapped.lineTotal ~> $sum(),
    "cheapSkus": orders.items[price < 10].sku,
    "regionKeys": $keys($regionObj),
    "nestedProjection": $mapped ~> $map(function($m){(
      $t := $m.lineTotal;
      {"id": $m.id, "bucket": $t >= 100 ? "big" : "small"}
    )})
  }
)
"#;

impl JsonCallable for DoubleCallable {
    fn call(
        &self,
        _ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
        let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
        Box::pin(async move {
            if let JsonValue::Number(value) = input {
                return Ok(JsonValue::Number(value * 2.0));
            }
            Ok(JsonValue::Undefined)
        })
    }

    fn arity(&self) -> Option<usize> {
        Some(1)
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}

impl JsonCallable for YieldOnceDoubleCallable {
    fn call(
        &self,
        _ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
        let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
        let pending_polls = Arc::clone(&self.pending_polls);
        let mut yielded_once = false;
        Box::pin(futures::future::poll_fn(move |cx| {
            if !yielded_once {
                yielded_once = true;
                pending_polls.fetch_add(1, Ordering::Relaxed);
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            if let JsonValue::Number(value) = input {
                return Poll::Ready(Ok(JsonValue::Number(value * 2.0)));
            }
            Poll::Ready(Ok(JsonValue::Undefined))
        }))
    }

    fn arity(&self) -> Option<usize> {
        Some(1)
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}

#[test]
fn parser_accepts_basic_expression() {
    let ast = parser::parse_expression("Account.Order[0].Product", false)
        .expect("parser should produce AST");
    assert!(ast.is_object());
    assert!(ast.get("type").is_some());
}

#[test]
fn parser_reports_syntax_error() {
    let error = parser::parse_expression("1+", false).expect_err("parser should fail");
    assert!(!error.code.is_empty());
    assert!(error.position > 0);
}

#[test]
fn parser_accepts_complex_everything_expression() {
    let ast = parser::parse_expression(COMPLEX_ALL_IN_EXPR, false)
        .expect("complex expression should be parsed");
    assert!(ast.is_object());
    assert!(ast.get("type").is_some());

    // Golden expected output from reference JSONata engine.
    // This is stored as an explicit fixture so we can use it once Rust evaluation lands.
    let expected = json!({
      "selectedCount": 2,
      "idsViaChain": ["A1", "C3"],
      "mapped": [
        {
          "id": "A1",
          "customer": "ALICE",
          "lineTotal": 30,
          "positiveItemCount": 2,
          "firstSku": "p1"
        },
        {
          "id": "C3",
          "customer": "CAROL",
          "lineTotal": 124,
          "positiveItemCount": 2,
          "firstSku": "p4"
        }
      ],
      "sortedIdsByTotalDesc": ["C3", "A1"],
      "grandTotal": 154,
      "cheapSkus": ["p2", "p4"],
      "regionKeys": ["eu-west", "eu-east"],
      "nestedProjection": [
        {
          "id": "A1",
          "bucket": "small"
        },
        {
          "id": "C3",
          "bucket": "big"
        }
      ]
    });
    assert_eq!(expected["selectedCount"], 2);
    assert_eq!(expected["grandTotal"], 154);
    assert_eq!(expected["nestedProjection"][1]["bucket"], "big");
}

#[test]
fn math_sum_jsonata_array() {
    let input = JsonataValue::Array(JsonataArray::new(
        vec![
            JsonataValue::Number(1.0),
            JsonataValue::Number(2.0),
            JsonataValue::Number(3.0),
        ],
        true,
        false,
    ));
    let result = math::sum_jsonata(&input).expect("sum should succeed");
    assert!(matches!(result, JsonataValue::Number(value) if value == 6.0));
}

#[test]
fn strings_substring_smoke() {
    let value = JsonValue::String("jsonata".to_owned());
    let start = JsonValue::Number(2.0);
    let length = JsonValue::Number(3.0);
    let result = strings::substring(&value, &start, &length).expect("substring should succeed");
    assert_eq!(result, JsonValue::String("ona".to_owned()));
}

#[test]
fn strings_format_number_smoke() {
    let result = strings::format_number(
        &JsonValue::Number(12345.6),
        &JsonValue::String("#,###.00".to_owned()),
        &JsonValue::Undefined,
    )
    .expect("formatNumber should succeed");
    assert_eq!(result, JsonValue::String("12,345.60".to_owned()));
}

#[test]
fn core_map_with_rust_callable() {
    let callable = JsonValue::Function(JsonFunction::new(Arc::new(DoubleCallable)));
    let input = JsonValue::Array(JsonArray::new(
        vec![
            JsonValue::Number(1.0),
            JsonValue::Number(2.0),
            JsonValue::Number(3.0),
        ],
        true,
        false,
    ));
    let result =
        block_on(core::map(FunctionContext::empty(), input, callable)).expect("map should succeed");
    assert_eq!(
        result,
        JsonValue::Array(JsonArray::new(
            vec![
                JsonValue::Number(2.0),
                JsonValue::Number(4.0),
                JsonValue::Number(6.0),
            ],
            true,
            false,
        ))
    );
}

#[test]
fn core_map_with_truly_async_callable() {
    let pending_polls = Arc::new(AtomicUsize::new(0));
    let callable = JsonValue::Function(JsonFunction::new(Arc::new(YieldOnceDoubleCallable {
        pending_polls: Arc::clone(&pending_polls),
    })));
    let input = JsonValue::Array(JsonArray::new(
        vec![
            JsonValue::Number(1.0),
            JsonValue::Number(2.0),
            JsonValue::Number(3.0),
        ],
        true,
        false,
    ));

    let result = block_on(core::map(FunctionContext::empty(), input, callable))
        .expect("async map should succeed");
    assert_eq!(
        result,
        JsonValue::Array(JsonArray::new(
            vec![
                JsonValue::Number(2.0),
                JsonValue::Number(4.0),
                JsonValue::Number(6.0),
            ],
            true,
            false,
        ))
    );
    assert_eq!(pending_polls.load(Ordering::Relaxed), 3);
}
