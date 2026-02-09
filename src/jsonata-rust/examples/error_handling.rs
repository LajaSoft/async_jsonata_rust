use async_jsonata_rust::functions::math;
use async_jsonata_rust::Parser;

fn main() {
    match Parser::new().parse("1+") {
        Ok(_) => println!("Unexpected parse success"),
        Err(err) => println!("Parse error: {} at {:?}", err.code(), err.position()),
    }

    match math::sqrt(Some(-1.0)) {
        Ok(value) => println!("Unexpected sqrt result: {:?}", value),
        Err(err) => println!("Runtime error: {}: {}", err.code, err.message),
    }
}
