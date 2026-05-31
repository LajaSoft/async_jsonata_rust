use std::fs;
use std::path::Path;

use async_jsonata_rust::Parser;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct GoldenCase {
    id: String,
    expr: String,
}

fn load_cases() -> Vec<GoldenCase> {
    let mut cases = Vec::new();
    let dir = Path::new("tests/golden");

    for entry in fs::read_dir(dir).expect("golden directory should exist") {
        let entry = entry.expect("golden dir entry should be readable");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let content = fs::read_to_string(&path).expect("golden fixture should be readable");
        let case: GoldenCase =
            serde_json::from_str(&content).expect("golden fixture should be valid JSON");
        cases.push(case);
    }

    cases.sort_by(|a, b| a.id.cmp(&b.id));
    cases
}

#[test]
fn golden_expressions_parse_successfully() {
    let parser = Parser::new();
    let cases = load_cases();

    assert!(!cases.is_empty(), "golden cases should not be empty");
    for case in cases {
        let expr = parser
            .parse(case.expr.as_str())
            .unwrap_or_else(|err| panic!("{} failed to parse: {}", case.id, err));
        assert!(
            expr.ast().is_object(),
            "{} should produce object AST",
            case.id
        );
    }
}

// The differential test that cross-checked golden cases against the upstream
// jsonata-js runtime via node was removed along with the JS sources; the pure
// Rust engine is now validated end-to-end by tests/official_suite.rs.
