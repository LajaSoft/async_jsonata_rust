//! Regression tests for the per-evaluation recursion **budget**.
//!
//! The evaluator bounds runaway recursion with two limits — a non-tail call
//! depth and a tail-call step count — plus on-demand native-stack growth. These
//! limits used to be process-/thread-scoped constants; they are now owned by the
//! evaluation via [`async_jsonata_rust::EvaluatorOptions`] and an internal
//! `Budget`. These tests pin the two behaviours that change made possible:
//!
//! 1. The limits are configurable (and can be disabled), and
//! 2. The non-tail depth guard is correct under `evaluate_async` when a single
//!    evaluation's future **migrates between executor threads** — the case a
//!    `thread_local!` counter got wrong (increment on one thread, drop on
//!    another → drift → spurious `U1001`).

use std::any::Any;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::Poll;

use async_jsonata_rust::types::{FunctionContext, JsonCallable, JsonError, JsonFunction, JsonValue};
use async_jsonata_rust::{Evaluator, FunctionRegistry, Parser};
use futures::executor::{block_on, ThreadPool};
use futures::future::BoxFuture;
use futures::task::SpawnExt;

// ---------------------------------------------------------------------------
// Configurable limits (#3)
// ---------------------------------------------------------------------------

/// A non-tail recursive sum: `$rec($n-1)` is an operand of `+`, so each level
/// stays live on the stack (counts against the non-tail depth guard).
const NON_TAIL_SUM: &str =
    "($rec := function($n){ $n = 0 ? 0 : $rec($n - 1) + $n }; $rec(200))";

/// A tail recursive countdown: the recursive call is the whole `then` branch, so
/// it is tail-call optimised and only counts against the tail-step backstop.
const TAIL_COUNTDOWN: &str =
    "($count := function($n){ $n = 0 ? 0 : $count($n - 1) }; $count(200))";

#[test]
fn non_tail_depth_limit_is_configurable() {
    let expression = Parser::new().parse(NON_TAIL_SUM).expect("parse");

    // A limit below the recursion depth aborts with the stack-overflow code.
    let limited = Evaluator::with_builtins().with_max_non_tail_depth(Some(50));
    let error = limited
        .evaluate(&expression, &JsonValue::Null)
        .expect_err("recursion of depth 200 must exceed a depth-50 budget");
    assert_eq!(error.code(), "U1001");

    // A limit comfortably above the depth lets the same expression complete.
    let generous = Evaluator::with_builtins().with_max_non_tail_depth(Some(100_000));
    let result = generous
        .evaluate(&expression, &JsonValue::Null)
        .expect("depth 200 is within a depth-100000 budget");
    assert_eq!(result, JsonValue::Number(20_100.0)); // sum(1..=200)
}

#[test]
fn tail_call_step_limit_is_configurable() {
    let expression = Parser::new().parse(TAIL_COUNTDOWN).expect("parse");

    // Fewer permitted steps than the chain length trips the backstop.
    let limited = Evaluator::with_builtins().with_max_tail_call_steps(Some(50));
    let error = limited
        .evaluate(&expression, &JsonValue::Null)
        .expect_err("200 tail steps must exceed a 50-step budget");
    assert_eq!(error.code(), "U1001");

    // `None` disables the backstop entirely: the chain runs to completion.
    let unbounded = Evaluator::with_builtins().with_max_tail_call_steps(None);
    let result = unbounded
        .evaluate(&expression, &JsonValue::Null)
        .expect("an unbounded tail budget never caps a terminating recursion");
    assert_eq!(result, JsonValue::Number(0.0));
}

#[test]
fn deep_tail_recursion_runs_in_constant_native_stack() {
    // 50k tail steps is far deeper than any fixed stack could hold, yet the
    // trampoline drives it in O(1) native stack. A tiny base sync stack proves
    // the tail path does not consume stack per step.
    let expression = Parser::new()
        .parse("($count := function($n){ $n = 0 ? 0 : $count($n - 1) }; $count(50000))")
        .expect("parse");
    let evaluator = Evaluator::with_builtins().with_sync_stack_size(128 * 1024);
    let result = evaluator
        .evaluate(&expression, &JsonValue::Null)
        .expect("deep tail recursion must not overflow the stack");
    assert_eq!(result, JsonValue::Number(0.0));
}

#[test]
fn deep_non_tail_recursion_grows_the_stack_on_demand() {
    // Non-tail recursion genuinely consumes native stack; depth 1500 dwarfs the
    // 256 KiB base sync stack and only succeeds because the evaluator grows the
    // stack in segments on demand (#4).
    let expression = Parser::new()
        .parse("($rec := function($n){ $n = 0 ? 0 : $rec($n - 1) + $n }; $rec(1500))")
        .expect("parse");
    let evaluator = Evaluator::with_builtins().with_sync_stack_size(256 * 1024);
    let result = evaluator
        .evaluate(&expression, &JsonValue::Null)
        .expect("on-demand stack growth must absorb depth-1500 non-tail recursion");
    assert_eq!(result, JsonValue::Number(1_125_750.0)); // sum(1..=1500)
}

#[test]
fn mutual_recursion_resolves() {
    // `$isEven`/`$isOdd` are defined in the same block and reference each other
    // (let-rec knot-tying); each call is in tail position.
    let expression = Parser::new()
        .parse(
            "(\
               $isEven := function($n){ $n = 0 ? true : $isOdd($n - 1) };\
               $isOdd := function($n){ $n = 0 ? false : $isEven($n - 1) };\
               [$isEven(1000), $isOdd(1000)]\
             )",
        )
        .expect("parse");
    let result = Evaluator::with_builtins()
        .evaluate(&expression, &JsonValue::Null)
        .expect("mutual recursion must resolve");
    let json = result.to_serde_json().expect("array result");
    assert_eq!(json, serde_json::json!([true, false]));
}

// ---------------------------------------------------------------------------
// Thread-migration correctness (#1)
// ---------------------------------------------------------------------------

/// An async callable that yields `Poll::Pending` exactly once (waking itself)
/// before returning its numeric argument unchanged. The single yield is a
/// suspension point at which a work-stealing executor may resume the evaluation
/// on a *different* thread.
#[derive(Clone)]
struct YieldOncePassthrough {
    yields: Arc<AtomicUsize>,
}

impl JsonCallable for YieldOncePassthrough {
    fn call(
        &self,
        _ctx: FunctionContext,
        args: Vec<JsonValue>,
    ) -> BoxFuture<'static, Result<JsonValue, JsonError>> {
        let input = args.first().cloned().unwrap_or(JsonValue::Undefined);
        let yields = Arc::clone(&self.yields);
        let mut yielded = false;
        Box::pin(futures::future::poll_fn(move |cx| {
            if !yielded {
                yielded = true;
                yields.fetch_add(1, Ordering::Relaxed);
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            Poll::Ready(Ok(input.clone()))
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
fn async_non_tail_recursion_survives_thread_migration() {
    // Non-tail recursion that awaits a yielding async fn at every level, so the
    // depth guard is entered and (eventually) dropped across suspension points.
    // On a multi-thread work-stealing pool a single evaluation's future is
    // resumed on whichever worker is free, so guard enter/drop routinely land on
    // different threads. With the old thread-local counter, 64 such evaluations
    // sharing 4 workers would corrupt each other's depth and raise spurious
    // `U1001`; with a per-evaluation budget every result is correct.
    let yields = Arc::new(AtomicUsize::new(0));
    let mut registry = FunctionRegistry::with_builtins();
    registry.insert(
        "slow",
        JsonFunction::new(Arc::new(YieldOncePassthrough {
            yields: Arc::clone(&yields),
        })),
    );
    let evaluator = Evaluator::new(registry);

    // `TASKS * DEPTH / workers` (≈ 4000) sits well above the default 2500 depth
    // budget, so the old shared-per-thread counter would overflow; kept modest
    // because resuming a depth-`DEPTH` future at every yield is O(DEPTH) work.
    const DEPTH: usize = 250;
    const TASKS: usize = 64;
    let expression = Parser::new()
        .parse(&format!(
            "($rec := function($n){{ $n = 0 ? 0 : $slow($n) + $rec($n - 1) }}; $rec({DEPTH}))"
        ))
        .expect("parse");
    let expected = JsonValue::Number((DEPTH * (DEPTH + 1) / 2) as f64); // sum(1..=DEPTH)

    let pool = ThreadPool::builder()
        .pool_size(4)
        .create()
        .expect("thread pool");

    let handles: Vec<_> = (0..TASKS)
        .map(|_| {
            let evaluator = evaluator.clone();
            let expression = expression.clone();
            pool.spawn_with_handle(async move {
                evaluator
                    .evaluate_async(&expression, &JsonValue::Null)
                    .await
            })
            .expect("spawn")
        })
        .collect();

    let results = block_on(futures::future::join_all(handles));
    for (task, result) in results.into_iter().enumerate() {
        let value = result.unwrap_or_else(|err| panic!("task {task} failed: {err:?}"));
        assert_eq!(value, expected, "task {task} produced the wrong sum");
    }

    // Every level of every task yielded once: proof the futures actually
    // suspended (and thus had the opportunity to migrate).
    assert_eq!(yields.load(Ordering::Relaxed), TASKS * DEPTH);
}

#[test]
fn concurrent_evaluations_have_isolated_depth_budgets() {
    // One evaluation with a deliberately tiny depth budget must fail without
    // disturbing many concurrent evaluations running under the default budget:
    // the counters belong to the evaluation, not the process.
    let generous = Parser::new().parse(NON_TAIL_SUM).expect("parse"); // depth 200
    let ok_evaluator = Evaluator::with_builtins();
    let tight_evaluator = Evaluator::with_builtins().with_max_non_tail_depth(Some(10));

    let pool = ThreadPool::builder()
        .pool_size(4)
        .create()
        .expect("thread pool");

    let mut handles = Vec::new();
    for _ in 0..32 {
        let evaluator = ok_evaluator.clone();
        let expression = generous.clone();
        handles.push(
            pool.spawn_with_handle(async move {
                evaluator
                    .evaluate_async(&expression, &JsonValue::Null)
                    .await
            })
            .expect("spawn"),
        );
    }
    // Interleave the tight-budget evaluation that is expected to overflow.
    let overflow = tight_evaluator
        .evaluate(&generous, &JsonValue::Null)
        .expect_err("a depth-10 budget cannot hold depth-200 recursion");
    assert_eq!(overflow.code(), "U1001");

    for (task, result) in block_on(futures::future::join_all(handles))
        .into_iter()
        .enumerate()
    {
        let value = result.unwrap_or_else(|err| panic!("task {task} failed: {err:?}"));
        assert_eq!(value, JsonValue::Number(20_100.0), "task {task} wrong");
    }
}
