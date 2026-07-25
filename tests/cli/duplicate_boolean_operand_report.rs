use super::*;

#[test]
fn cli_reports_a_repeated_operand_in_an_or() {
    let dir = fresh_temp_dir("duplicate-boolean-operand-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(or x y x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-boolean-operands")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 1"))
        .stdout(predicate::str::contains("\"operand\": \"x\""));
}

#[test]
fn cli_finds_a_boolean_nested_in_a_function_body() {
    let dir = fresh_temp_dir("duplicate-boolean-operand-report-nested");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (when (or (p x) (p x)) 1))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-boolean-operands")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 1"));
}

#[test]
fn cli_does_not_flag_distinct_operands() {
    let dir = fresh_temp_dir("duplicate-boolean-operand-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(and a b c)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-boolean-operands")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 0"));
}

#[test]
fn cli_duplicate_boolean_operands_fail_on_duplicate_trips_gate() {
    let dir = fresh_temp_dir("duplicate-boolean-operand-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(or x x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-boolean-operands")
        .arg("--fail-on-duplicate")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "duplicate-boolean-operand-report policy failed",
        ));
}

#[test]
fn cli_duplicate_boolean_operands_passes_gate_when_all_operands_are_distinct() {
    let dir = fresh_temp_dir("duplicate-boolean-operand-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(or a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-boolean-operands")
        .arg("--fail-on-duplicate")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 0"));
}
