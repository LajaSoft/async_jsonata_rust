use async_jsonata_rust::Parser;

#[test]
fn bug_0001_trailing_operator_reports_error() {
    let err = Parser::new()
        .parse("1+")
        .expect_err("parser should reject trailing operator");
    assert_eq!(err.code(), "S0207");
}

#[test]
fn bug_0002_parent_selector_is_rejected_postprocess() {
    let err = Parser::new()
        .parse("%")
        .expect_err("standalone parent selector should fail");
    assert_eq!(err.code(), "S0217");
}

#[test]
fn bug_0003_recover_mode_emits_errors_array() {
    let expr = Parser::new()
        .with_recover(true)
        .parse("1+")
        .expect("recover parser should return AST");
    assert!(expr.ast().get("errors").is_some());
}
