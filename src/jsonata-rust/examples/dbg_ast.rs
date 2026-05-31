use async_jsonata_rust::Evaluator;

fn main() {
    let ev = Evaluator::with_builtins();
    for e in std::env::args().skip(1) {
        let p = ev.parse(e.clone()).unwrap();
        println!("=== {e} ===");
        println!("{}", serde_json::to_string_pretty(p.ast()).unwrap());
    }
}
