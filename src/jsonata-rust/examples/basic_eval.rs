use async_jsonata_rust::types::{FunctionContext, JsonValue};
use async_jsonata_rust::{Evaluator, FunctionRegistry};
use futures::executor::block_on;

fn main() {
    let evaluator = Evaluator::with_builtins();
    match evaluator.parse("$sqrt(81)") {
        Ok(expr) => {
            println!("Source: {}", expr.source());
            println!("AST type: {}", expr.ast()["type"]);

            match evaluator.evaluate(&expr, &JsonValue::Null) {
                Ok(value) => println!("Evaluator result: {:?}", value),
                Err(err) => println!("Evaluator status: {}: {}", err.code(), err.message()),
            }

            let registry = FunctionRegistry::with_builtins();
            let sqrt = registry.get("sqrt").expect("sqrt should exist").clone();
            let value = block_on(sqrt.call(
                FunctionContext::empty(),
                vec![JsonValue::Number(81.0)],
            ))
            .expect("sqrt should evaluate");
            println!("Runtime built-in $sqrt(81) = {:?}", value);
        }
        Err(err) => {
            eprintln!("{}: {}", err.code(), err.message());
            std::process::exit(1);
        }
    }
}
