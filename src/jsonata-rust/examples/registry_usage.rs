use futures::executor::block_on;
use async_jsonata_rust::create_builtin_registry;
use async_jsonata_rust::types::{FunctionContext, JsonValue};

fn main() {
    let registry = create_builtin_registry();
    let sqrt = registry.get("sqrt").expect("sqrt must exist").clone();

    let result = block_on(sqrt.call(FunctionContext::empty(), vec![JsonValue::Number(81.0)]))
        .expect("sqrt should succeed");

    println!("sqrt(81) = {:?}", result);
}
