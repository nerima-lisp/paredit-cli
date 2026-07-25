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
        .stdout(predicate::str::contains("\"violation_count\": 1"))
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
        .stdout(predicate::str::contains("\"violation_count\": 0"));
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
        .stdout(predicate::str::contains("\"violation_count\": 0"));
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
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}
