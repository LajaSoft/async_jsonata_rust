use jsonata_rust::functions::math;
use jsonata_rust::parse_expression;

fn main() {
    match math::sqrt(Some(-1.0)) {
        Ok(value) => println!("Unexpected sqrt result: {:?}", value),
        Err(err) => println!("Runtime error: {}: {}", err.code, err.message),
    }

    match parse_expression("1+", false) {
        Ok(_) => println!("Unexpected parse success"),
        Err(err) => println!("Parser error: {} at {}", err.code, err.position),
    }
}
