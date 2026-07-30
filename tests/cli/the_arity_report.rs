use super::*;

#[test]
fn cli_reports_too_few_arguments() {
    let dir = fresh_temp_dir("the-arity-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(the fixnum)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("the-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"the_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"argument_count\": 1"));
}

#[test]
fn cli_reports_too_many_arguments() {
    let dir = fresh_temp_dir("the-arity-report-many");
    let file = dir.join("a.lisp");
    fs::write(&file, "(the fixnum x y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("the-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"argument_count\": 3"));
}

#[test]
fn cli_does_not_flag_a_valid_the() {
    let dir = fresh_temp_dir("the-arity-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(the fixnum (+ a b))\n(the string s)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("the-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "both `the` forms are well-formed"
        // from "there is no `the` form at all".
        .stdout(predicate::str::contains("\"the_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_does_not_flag_a_reader_conditional_type() {
    let dir = fresh_temp_dir("the-arity-report-feature");
    let file = dir.join("a.lisp");
    fs::write(&file, "(the #+sbcl fixnum #-sbcl integer x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("the-arity")
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
    let dir = fresh_temp_dir("the-arity-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [] (the fixnum))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "the-arity", "--output", "json"])
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
fn cli_the_arity_emits_sarif() {
    let dir = fresh_temp_dir("the-arity-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(the fixnum)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "the-arity", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/the-arity/the-arity\"",
        ))
        .stdout(predicate::str::contains(
            "the takes exactly 2 arguments (a type and a form) but has 1",
        ));
}

#[test]
fn cli_the_arity_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("the-arity-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(the fixnum)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("the-arity")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("the-arity-report policy failed"));
}

#[test]
fn cli_the_arity_expands_directory_inputs() {
    let dir = fresh_temp_dir("the-arity-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (x) (the fixnum x x))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("the-arity")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
