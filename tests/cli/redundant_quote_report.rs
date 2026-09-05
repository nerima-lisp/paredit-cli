use super::*;

#[test]
fn cli_reports_a_quoted_number_and_keyword() {
    let dir = fresh_temp_dir("redundant-quote-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defparameter *n* '5)\n(list ':foo)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-quote")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 2"))
        .stdout(predicate::str::contains("\"kind\": \"number\""))
        .stdout(predicate::str::contains("\"kind\": \"keyword\""))
        .stdout(predicate::str::contains("\"literal\": \"5\""))
        .stdout(predicate::str::contains("\"literal\": \":foo\""))
        .stdout(predicate::str::contains("\"line\": 2"));
}

#[test]
fn cli_does_not_flag_quoted_symbols_or_lists() {
    let dir = fresh_temp_dir("redundant-quote-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list 'foo 't 'nil '() '(1 2 3))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-quote")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no redundant quote among five
        // quoted forms" from "no quoted form at all".
        .stdout(predicate::str::contains("\"quoted_form_count\": 5"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("redundant-quote-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(list '5)\n").expect("write a.clj");

    paredit()
        .args(["inspect", "redundant-quote", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_redundant_quote_emits_sarif() {
    let dir = fresh_temp_dir("redundant-quote-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defparameter *n* '5)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "redundant-quote", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/redundant-quote/number\"",
        ))
        .stdout(predicate::str::contains("quoting number 5 is redundant"));
}

#[test]
fn cli_redundant_quote_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-quote-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(princ '\"literal\")\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-quote")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-quote-report policy failed",
        ));
}

#[test]
fn cli_redundant_quote_passes_gate_when_clean() {
    let dir = fresh_temp_dir("redundant-quote-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(member x '(1 2 3))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-quote")
        .arg("--fail-on-violation")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_redundant_quote_expands_directory_inputs() {
    let dir = fresh_temp_dir("redundant-quote-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(list '#\\a)\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-quote")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"kind\": \"character\""));
}
