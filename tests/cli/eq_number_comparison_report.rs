use super::*;

#[test]
fn cli_reports_eq_against_a_number_literal() {
    let dir = fresh_temp_dir("eq-number-comparison-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when (eq count 0) :empty)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eq-number-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"comparison_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"literal\": \"0\""));
}

#[test]
fn cli_does_not_flag_eql_or_numeric_equality() {
    let dir = fresh_temp_dir("eq-number-comparison-report-ok");
    let file = dir.join("a.lisp");
    fs::write(&file, "(and (eql n 5) (= n 5))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eq-number-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_does_not_flag_the_increment_function_symbol() {
    let dir = fresh_temp_dir("eq-number-comparison-report-symbol");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq op '1+)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eq-number-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no number argument in one `eq`"
        // from "no `eq` at all".
        .stdout(predicate::str::contains("\"comparison_form_count\": 1"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("eq-number-comparison-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn empty? [n] (= n 0))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "eq-number-comparison", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

/// The envelope's interchange formats, which this report reached by moving onto
/// it. Asserted here only far enough to prove the command accepts them; their
/// content is covered once in `report_interop`.
#[test]
fn cli_eq_number_comparison_emits_sarif() {
    let dir = fresh_temp_dir("eq-number-comparison-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq n 5)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "eq-number-comparison", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/eq-number-comparison/eq-number-comparison\"",
        ))
        .stdout(predicate::str::contains(
            "eq compares against number literal 5",
        ));
}

#[test]
fn cli_eq_number_comparison_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("eq-number-comparison-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq n 5)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eq-number-comparison")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "eq-number-comparison-report policy failed",
        ));
}

#[test]
fn cli_eq_number_comparison_passes_gate_when_clean() {
    let dir = fresh_temp_dir("eq-number-comparison-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq x y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eq-number-comparison")
        .arg("--fail-on-violation")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}
