use super::*;

#[test]
fn cli_reports_a_situation_typo() {
    let dir = fresh_temp_dir("eval-when-situation-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eval-when (:compile-toplevel :executee) 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eval-when-situation")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"eval_when_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"situation\": \":executee\""));
}

#[test]
fn cli_does_not_flag_valid_situations() {
    let dir = fresh_temp_dir("eval-when-situation-report-clean");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(eval-when (:compile-toplevel :load-toplevel :execute) 1)\n(eval-when (compile load eval) 2)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eval-when-situation")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "every situation in two eval-when
        // forms is valid" from "no eval-when form at all".
        .stdout(predicate::str::contains("\"eval_when_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_does_not_flag_a_reader_conditional_situation() {
    let dir = fresh_temp_dir("eval-when-situation-report-feature");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eval-when (#+sbcl :execute) 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eval-when-situation")
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
fn cli_eval_when_situation_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("eval-when-situation-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(eval-when (:bogus) 1)\n").expect("write a.clj");

    paredit()
        .args(["inspect", "eval-when-situation", "--output", "json"])
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
fn cli_eval_when_situation_emits_sarif() {
    let dir = fresh_temp_dir("eval-when-situation-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eval-when (:bogus) 1)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "eval-when-situation", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/eval-when-situation/eval-when-situation\"",
        ))
        .stdout(predicate::str::contains(
            "eval-when situation :bogus is not valid",
        ));
}

#[test]
fn cli_eval_when_situation_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("eval-when-situation-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eval-when (:bogus) 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eval-when-situation")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "eval-when-situation-report policy failed",
        ));
}

#[test]
fn cli_eval_when_situation_expands_directory_inputs() {
    let dir = fresh_temp_dir("eval-when-situation-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(progn (eval-when (:loud) 1))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eval-when-situation")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
