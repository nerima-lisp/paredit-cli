use super::*;

#[test]
fn cli_reports_too_many_arguments() {
    let dir = fresh_temp_dir("if-arity-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if (plusp x) :pos :neg :zero)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"if_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"argument_count\": 4"));
}

#[test]
fn cli_does_not_flag_valid_if_forms() {
    let dir = fresh_temp_dir("if-arity-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if a b)\n(if a b c)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "both `if` forms here are well
        // formed" from "no `if` form at all".
        .stdout(predicate::str::contains("\"if_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_does_not_flag_a_feature_conditional_else() {
    let dir = fresh_temp_dir("if-arity-report-feature");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if a b #+sbcl c #-sbcl d)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // Skipped entirely rather than counted: the written arity is not the
        // evaluated one, so it is not a scanned form either.
        .stdout(predicate::str::contains("\"if_form_count\": 0"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("if-arity-report-unmodelled");
    let file = dir.join("a.el");
    fs::write(&file, "(if a b c d)\n").expect("write a.el");

    paredit()
        .args(["inspect", "if-arity", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_if_arity_emits_sarif() {
    let dir = fresh_temp_dir("if-arity-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if a b c d)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "if-arity", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/if-arity/if-arity\"",
        ))
        .stdout(predicate::str::contains(
            "if takes 2 or 3 arguments but has 4",
        ));
}

#[test]
fn cli_if_arity_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("if-arity-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if a)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-arity")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("if-arity-report policy failed"));
}

#[test]
fn cli_if_arity_expands_directory_inputs() {
    let dir = fresh_temp_dir("if-arity-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (x) (if x 1 2 3))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-arity")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"argument_count\": 4"));
}
