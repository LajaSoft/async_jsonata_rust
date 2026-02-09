use async_jsonata_rust::{Evaluator, JsonValue};

fn main() {
    let evaluator = Evaluator::with_builtins();
    let expression = evaluator
        .parse(
            "(
                $double := function($x){$x * 2};
                [1,2,3] ~> $map(function($v){$double($v)})
            )",
        )
        .expect("expression should parse");

    let result = evaluator
        .evaluate(&expression, &JsonValue::Null)
        .expect("evaluation should succeed");

    println!("result: {result:?}");
}
