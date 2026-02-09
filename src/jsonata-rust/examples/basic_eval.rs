use jsonata_rust::Parser;

fn main() {
    let parser = Parser::new();
    match parser.parse("Account.Order[0].Product") {
        Ok(expr) => {
            println!("Source: {}", expr.source());
            println!("AST type: {}", expr.ast()["type"]);
        }
        Err(err) => {
            eprintln!("{}: {}", err.code(), err.message());
            std::process::exit(1);
        }
    }
}
