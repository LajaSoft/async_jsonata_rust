use jsonata_rust::parse_expression;

fn main() {
    let expression = "Account.Order[0].Product";
    match parse_expression(expression, false) {
        Ok(ast) => {
            println!("Parsed expression: {expression}");
            println!("AST node type: {}", ast["type"]);
        }
        Err(err) => {
            eprintln!("Parser error {} at position {}", err.code, err.position);
            std::process::exit(1);
        }
    }
}
