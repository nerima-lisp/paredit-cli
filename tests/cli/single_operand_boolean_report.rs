use super::*;

#[test]
fn cli_flags_single_operand_and() {
    let dir = fresh_temp_dir("single-operand-boolean-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (and x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-operand-boolean")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"boolean_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"and\""));
}

#[test]
fn cli_flags_single_operand_or() {
    let dir = fresh_temp_dir("single-operand-boolean-report-or");
    let file = dir.join("a.lisp");
    fs::write(&file, "(or (compute))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-operand-boolean")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"or\""));
}

#[test]
fn cli_does_not_flag_multi_operand_or_empty() {
    let dir = fresh_temp_dir("single-operand-boolean-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(and x y)\n(or)\n(and)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-operand-boolean")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no single-operand form among three
        // booleans" from "no boolean form at all".
        .stdout(predicate::str::contains("\"boolean_form_count\": 3"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_does_not_flag_a_lone_reader_conditional() {
    let dir = fresh_temp_dir("single-operand-boolean-report-feature");
    let file = dir.join("a.lisp");
    fs::write(&file, "(and #+sbcl (sb-only))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-operand-boolean")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("single-operand-boolean-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn check [x] (and x))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "single-operand-boolean", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_single_operand_boolean_emits_sarif() {
    let dir = fresh_temp_dir("single-operand-boolean-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(or x)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "single-operand-boolean", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/single-operand-boolean/or\"",
        ))
        .stdout(predicate::str::contains(
            "or has a single operand; (or X) is just X",
        ));
}

#[test]
fn cli_single_operand_boolean_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("single-operand-boolean-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(or x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-operand-boolean")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "single-operand-boolean-report policy failed",
        ));
}

#[test]
fn cli_single_operand_boolean_expands_directory_inputs() {
    let dir = fresh_temp_dir("single-operand-boolean-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (y) (or y))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-operand-boolean")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
