use super::*;

#[test]
fn cli_reports_a_list_place() {
    let dir = fresh_temp_dir("setq-non-variable-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq (car x) 5)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("setq-non-variable")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"setq\""))
        .stdout(predicate::str::contains("(car x)"));
}

#[test]
fn cli_reports_a_constant_place() {
    let dir = fresh_temp_dir("setq-non-variable-report-const");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq :k 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("setq-non-variable")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_valid_setq() {
    let dir = fresh_temp_dir("setq-non-variable-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq x 1 y 2)\n(setq *g* 3)\n(setf (car z) 9)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("setq-non-variable")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_does_not_flag_a_reader_conditional_form() {
    let dir = fresh_temp_dir("setq-non-variable-report-feature");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq #+sbcl a #-sbcl b 5)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("setq-non-variable")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_setq_non_variable_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("setq-non-variable-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq 5 x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("setq-non-variable")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "setq-non-variable-report policy failed",
        ));
}

#[test]
fn cli_setq_non_variable_expands_directory_inputs() {
    let dir = fresh_temp_dir("setq-non-variable-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (x) (setq (car x) 1))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("setq-non-variable")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}
