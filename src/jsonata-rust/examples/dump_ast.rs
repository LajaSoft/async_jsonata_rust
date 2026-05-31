use async_jsonata_rust::Parser;
fn main() {
    let exprs = vec![
        "Account.Order.Product[%.OrderID='order104'].SKU",
        "library.loans@$L.books@$B[$L.isbn=$B.isbn].SKU",
        "Account.Order.Product.SKU^(%.Price)",
    ];
    for e in exprs {
        match Parser::new().parse(e.to_string()) {
          Ok(p)=> { println!("=== {} ===", e); println!("{}", serde_json::to_string_pretty(p.ast()).unwrap()); }
          Err(err)=> println!("=== {} ERR {:?} ===", e, err),
        }
    }
}
