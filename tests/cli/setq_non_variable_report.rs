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
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"assignment_form_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"setq\""))
        .stdout(predicate::str::contains("(car x)"))
        .stdout(predicate::str::contains("\"line\": 1"));
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
        .stdout(predicate::str::contains("\"finding_count\": 1"));
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
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator counts the two `setq` forms scanned; the `setf` is
        // not this rule's business and is not counted.
        .stdout(predicate::str::contains("\"assignment_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
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
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("setq-non-variable-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(setq 5 x)\n").expect("write a.clj");

    paredit()
        .args(["inspect", "setq-non-variable", "--output", "json"])
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
fn cli_setq_non_variable_emits_sarif() {
    let dir = fresh_temp_dir("setq-non-variable-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq (car x) 5)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "setq-non-variable", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/setq-non-variable/setq-non-variable\"",
        ))
        .stdout(predicate::str::contains(
            "setq place (car x) is not a variable",
        ));
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
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
