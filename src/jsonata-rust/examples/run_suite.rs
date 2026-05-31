//! Runs the official JSONata test suite against the pure-Rust engine.
//!
//! Usage:
//!   cargo run --example run_suite                 # summary for every group
//!   cargo run --example run_suite -- <group>      # detailed failures for one group
//!   cargo run --example run_suite -- --failures   # summary + list every failing case
//!
//! The suite lives in `../jsonata/test/test-suite` relative to this crate.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use async_jsonata_rust::{Evaluator, JsonValue};
use serde_json::Value;

fn suite_root() -> PathBuf {
    // crate dir = src/jsonata-rust ; suite = src/jsonata/test/test-suite
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join("../jsonata/test/test-suite")
}

fn load_datasets(root: &Path) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    let dir = root.join("datasets");
    for entry in fs::read_dir(&dir).expect("datasets dir") {
        let path = entry.expect("dataset entry").path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let content = fs::read_to_string(&path).expect("read dataset");
        let value: Value = serde_json::from_str(&content).expect("parse dataset");
        out.insert(name, value);
    }
    out
}

#[derive(Debug)]
struct Case {
    description: String,
    expr: String,
    data: Option<Value>,
    bindings: serde_json::Map<String, Value>,
    expected: Expectation,
}

#[derive(Debug)]
enum Expectation {
    Result(Value),
    Undefined,
    Code(String),
}

fn resolve_data(spec: &Value, datasets: &HashMap<String, Value>) -> Option<Value> {
    if let Some(data) = spec.get("data") {
        return Some(data.clone());
    }
    match spec.get("dataset") {
        Some(Value::Null) | None => None,
        Some(Value::String(name)) => Some(
            datasets
                .get(name)
                .unwrap_or_else(|| panic!("unknown dataset {name}"))
                .clone(),
        ),
        Some(other) => panic!("unexpected dataset field: {other}"),
    }
}

fn parse_case(spec: &Value, group_dir: &Path, datasets: &HashMap<String, Value>) -> Case {
    let description = spec
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let expr = if let Some(file) = spec.get("expr-file").and_then(Value::as_str) {
        fs::read_to_string(group_dir.join(file)).expect("read expr-file")
    } else {
        spec.get("expr")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };

    let data = resolve_data(spec, datasets);

    let bindings = spec
        .get("bindings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let expected = if spec.get("undefinedResult").and_then(Value::as_bool) == Some(true) {
        Expectation::Undefined
    } else if let Some(result) = spec.get("result") {
        Expectation::Result(result.clone())
    } else if let Some(code) = spec.get("code").and_then(Value::as_str) {
        Expectation::Code(code.to_string())
    } else if let Some(err) = spec.get("error").and_then(Value::as_object) {
        let code = err
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        Expectation::Code(code)
    } else {
        // Nothing to test - treat as undefined expectation so it still runs.
        Expectation::Undefined
    };

    Case {
        description,
        expr,
        data,
        bindings,
        expected,
    }
}

fn load_group_cases(group_dir: &Path, datasets: &HashMap<String, Value>) -> Vec<Case> {
    let mut cases = Vec::new();
    let mut files: Vec<PathBuf> = fs::read_dir(group_dir)
        .expect("group dir")
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension().and_then(|s| s.to_str()) == Some("json")).then_some(p)
        })
        .collect();
    files.sort();
    for path in files {
        let content = fs::read_to_string(&path).expect("read case");
        let spec: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                // A few suite files embed lone UTF-16 surrogates which serde_json
                // (unlike JS JSON.parse) rejects. Skip them rather than abort.
                eprintln!("skip {}: {e}", path.display());
                continue;
            }
        };
        match spec {
            Value::Array(items) => {
                for item in &items {
                    cases.push(parse_case(item, group_dir, datasets));
                }
            }
            obj => cases.push(parse_case(&obj, group_dir, datasets)),
        }
    }
    cases
}

/// Scalar equality that treats JSON numbers by their f64 value, matching
/// JavaScript's single number type (so `10000000` equals `1e7` / `10000000.0`).
fn scalar_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(fx), Some(fy)) => fx == fy,
            _ => x == y,
        },
        _ => a == b,
    }
}

/// Order-sensitive deep equality (numbers compared by value).
fn ordered_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(i, j)| ordered_eq(i, j))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).map_or(false, |ov| ordered_eq(v, ov)))
        }
        _ => scalar_eq(a, b),
    }
}

/// Order-insensitive deep equality (multiset semantics for arrays).
fn unordered_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return false;
            }
            let mut used = vec![false; y.len()];
            for item in x {
                let mut found = false;
                for (i, other) in y.iter().enumerate() {
                    if !used[i] && unordered_eq(item, other) {
                        used[i] = true;
                        found = true;
                        break;
                    }
                }
                if !found {
                    return false;
                }
            }
            true
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).map_or(false, |ov| unordered_eq(v, ov)))
        }
        _ => scalar_eq(a, b),
    }
}

enum Outcome {
    Pass,
    Fail(String),
}

fn run_case(case: &Case) -> Outcome {
    let evaluator = Evaluator::with_builtins();
    let input = case
        .data
        .as_ref()
        .map(JsonValue::from_serde_json)
        .unwrap_or(JsonValue::Undefined);
    let bindings: HashMap<String, JsonValue> = case
        .bindings
        .iter()
        .map(|(k, v)| (k.clone(), JsonValue::from_serde_json(v)))
        .collect();

    let parsed = match evaluator.parse(case.expr.clone()) {
        Ok(p) => Ok(p),
        Err(e) => Err(e),
    };

    match (&case.expected, parsed) {
        (Expectation::Code(expected), Err(e)) => {
            if e.code() == expected {
                Outcome::Pass
            } else {
                Outcome::Fail(format!("expected error {expected}, parse error {}", e.code()))
            }
        }
        (_, Err(e)) => Outcome::Fail(format!("parse error {}: {}", e.code(), e.message())),
        (expectation, Ok(expr)) => {
            let result = evaluator.evaluate_with_bindings(&expr, &input, &bindings);
            match (expectation, result) {
                (Expectation::Code(expected), Err(e)) => {
                    if e.code() == expected {
                        Outcome::Pass
                    } else {
                        Outcome::Fail(format!("expected error {expected}, got {}", e.code()))
                    }
                }
                (Expectation::Code(expected), Ok(v)) => Outcome::Fail(format!(
                    "expected error {expected}, got value {:?}",
                    v.to_serde_json()
                )),
                (_, Err(e)) => {
                    Outcome::Fail(format!("unexpected error {}: {}", e.code(), e.message()))
                }
                (Expectation::Undefined, Ok(v)) => match v.to_serde_json() {
                    None => Outcome::Pass,
                    Some(json) => Outcome::Fail(format!("expected undefined, got {json}")),
                },
                (Expectation::Result(expected), Ok(v)) => {
                    let got = v.to_serde_json();
                    let matches = match &got {
                        Some(j) => ordered_eq(j, expected) || unordered_eq(j, expected),
                        None => expected.is_null(),
                    };
                    if matches {
                        Outcome::Pass
                    } else {
                        Outcome::Fail(format!(
                            "expected {expected}, got {}",
                            got.map(|j| j.to_string()).unwrap_or("undefined".into())
                        ))
                    }
                }
            }
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let show_failures = args.iter().any(|a| a == "--failures");
    let group_filter: Option<String> = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .cloned();

    let root = suite_root();
    let datasets = load_datasets(&root);
    let groups_dir = root.join("groups");

    let mut groups: Vec<String> = fs::read_dir(&groups_dir)
        .expect("groups dir")
        .filter_map(|e| {
            let p = e.ok()?.path();
            if p.is_dir() {
                Some(p.file_name()?.to_string_lossy().to_string())
            } else {
                None
            }
        })
        .collect();
    groups.sort();

    let mut total_pass = 0usize;
    let mut total = 0usize;
    let detailed = group_filter.is_some();

    for group in &groups {
        if let Some(filter) = &group_filter {
            if group != filter {
                continue;
            }
        }
        let cases = load_group_cases(&groups_dir.join(group), &datasets);
        let mut gp = 0usize;
        let mut failures: Vec<String> = Vec::new();
        for case in &cases {
            match run_case(case) {
                Outcome::Pass => gp += 1,
                Outcome::Fail(msg) => {
                    failures.push(format!("    [{}] {}\n      -> {}", case.description, case.expr, msg))
                }
            }
        }
        total_pass += gp;
        total += cases.len();
        let status = if gp == cases.len() { "OK" } else { "  " };
        println!("{status} {group}: {gp}/{}", cases.len());
        if (detailed || show_failures) && !failures.is_empty() {
            for f in &failures {
                println!("{f}");
            }
        }
    }

    println!("\n==== TOTAL: {total_pass}/{total} ====");
}
