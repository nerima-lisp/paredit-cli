use super::*;

#[test]
fn cli_reports_optional_after_key() {
    let dir = fresh_temp_dir("lambda-list-keyword-order-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (&key a &optional b) a)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lambda-list-keyword-order")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"definition_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"definition\": \"f\""))
        .stdout(predicate::str::contains("\"&optional\""))
        .stdout(predicate::str::contains("\"after_keyword\": \"&key\""));
}

#[test]
fn cli_does_not_flag_canonical_order() {
    let dir = fresh_temp_dir("lambda-list-keyword-order-report-clean");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(defun f (a &optional b &rest c &key d &allow-other-keys &aux (e 1)) a)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lambda-list-keyword-order")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "a canonically ordered lambda
        // list" from "no callable definition at all".
        .stdout(predicate::str::contains("\"definition_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_lambda_list_keyword_order_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("lambda-list-keyword-order-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [a b] a)\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "lambda-list-keyword-order", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_lambda_list_keyword_order_emits_sarif() {
    let dir = fresh_temp_dir("lambda-list-keyword-order-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (&key a &optional b) a)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "lambda-list-keyword-order", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/lambda-list-keyword-order/lambda-list-keyword-order\"",
        ))
        .stdout(predicate::str::contains(
            "f lists lambda-list keyword &optional after &key",
        ));
}

#[test]
fn cli_does_not_flag_a_body_lambda_list() {
    let dir = fresh_temp_dir("lambda-list-keyword-order-report-body");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defmacro m (&body b &optional o) b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lambda-list-keyword-order")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        // `&body` makes the lambda list unrankable, so the definition is
        // counted as scanned but produces no finding.
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        .stdout(predicate::str::contains("\"definition_count\": 1"));
}

#[test]
fn cli_lambda_list_keyword_order_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("lambda-list-keyword-order-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (&aux (x 1) &key k) x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lambda-list-keyword-order")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "lambda-list-keyword-order-report policy failed",
        ));
}

#[test]
fn cli_lambda_list_keyword_order_expands_directory_inputs() {
    let dir = fresh_temp_dir("lambda-list-keyword-order-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (&rest r &optional o) r)\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lambda-list-keyword-order")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
