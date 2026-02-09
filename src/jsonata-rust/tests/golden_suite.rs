use std::fs;
use std::path::Path;
use std::process::Command;

use jsonata_rust::Parser;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct GoldenCase {
    id: String,
    input: Value,
    expr: String,
    expected: Value,
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

fn eval_reference(expr: &str, input: &Value) -> Result<Value, String> {
    let script = r#"
const jsonata = require('../jsonata-js-rust/src/jsonata');
const input = JSON.parse(process.argv[1]);
const expr = process.argv[2];
(async () => {
  const out = await jsonata(expr).evaluate(input);
  process.stdout.write(JSON.stringify(out));
})().catch((err) => {
  const code = err && err.code ? err.code : 'ERR';
  const msg = err && err.message ? err.message : String(err);
  process.stderr.write(code + ':' + msg);
  process.exit(1);
});
"#;

    let output = Command::new("node")
        .arg("-e")
        .arg(script)
        .arg(input.to_string())
        .arg(expr)
        .output()
        .map_err(|err| format!("node exec failed: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.as_ref()).map_err(|err| format!("bad JSON output: {err}"))
}

#[test]
#[ignore = "requires node + jsonata-js reference runtime"]
fn differential_matches_jsonata_js_reference() {
    for case in load_cases() {
        let actual = eval_reference(case.expr.as_str(), &case.input)
            .unwrap_or_else(|err| panic!("{} reference eval failed: {}", case.id, err));
        assert_eq!(
            actual, case.expected,
            "{} mismatch against reference jsonata-js",
            case.id
        );
    }
}
