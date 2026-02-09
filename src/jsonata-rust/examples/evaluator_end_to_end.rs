use async_jsonata_rust::{Evaluator, JsonValue};

fn main() {
    let evaluator = Evaluator::with_builtins();
    let expression = evaluator
        .parse("$map(Account.Order, function($o){$o.Product})")
        .expect("expression should parse");

    let input = JsonValue::Object(async_jsonata_rust::JsonObject(vec![(
        "Account".to_string(),
        JsonValue::Object(async_jsonata_rust::JsonObject(vec![(
            "Order".to_string(),
            JsonValue::Array(async_jsonata_rust::JsonArray::new(
                vec![
                    JsonValue::Object(async_jsonata_rust::JsonObject(vec![(
                        "Product".to_string(),
                        JsonValue::String("Widget".to_string()),
                    )])),
                    JsonValue::Object(async_jsonata_rust::JsonObject(vec![(
                        "Product".to_string(),
                        JsonValue::String("Cable".to_string()),
                    )])),
                ],
                false,
                false,
            )),
        )])),
    )]));

    let result = evaluator
        .evaluate(&expression, &input)
        .expect("evaluation should succeed");

    println!("result: {result:?}");
}
