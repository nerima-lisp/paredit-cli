use super::*;

#[test]
fn cli_flags_add_zero() {
    let dir = fresh_temp_dir("identity-arithmetic-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (+ x 0))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("identity-arithmetic")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"arithmetic_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"+\""))
        .stdout(predicate::str::contains("\"identity\": \"0\""));
}

#[test]
fn cli_flags_multiply_one() {
    let dir = fresh_temp_dir("identity-arithmetic-report-mul");
    let file = dir.join("a.lisp");
    fs::write(&file, "(* n 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("identity-arithmetic")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"identity\": \"1\""));
}

#[test]
fn cli_does_not_flag_meaningful_arithmetic() {
    let dir = fresh_temp_dir("identity-arithmetic-report-clean");
    let file = dir.join("a.lisp");
    // (- 0 x) negates, (/ 1 x) is reciprocal, (* x 1.0) coerces, (+ x 2) is real.
    fs::write(&file, "(- 0 x)\n(/ 1 x)\n(* x 1.0)\n(+ x 2)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("identity-arithmetic")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no redundant operand in four
        // arithmetic forms" from "no arithmetic form at all".
        .stdout(predicate::str::contains("\"arithmetic_form_count\": 4"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("identity-arithmetic-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn same [x] (+ x 0))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "identity-arithmetic", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_identity_arithmetic_emits_sarif() {
    let dir = fresh_temp_dir("identity-arithmetic-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(+ x 0)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "identity-arithmetic", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        // The operators are punctuation, so the rule name is the kind and the
        // operator stays a property rather than part of the rule id.
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/identity-arithmetic/identity-arithmetic\"",
        ))
        .stdout(predicate::str::contains(
            "+ has a redundant identity operand 0 (it does nothing)",
        ));
}

#[test]
fn cli_identity_arithmetic_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("identity-arithmetic-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(/ total 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("identity-arithmetic")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "identity-arithmetic-report policy failed",
        ));
}

#[test]
fn cli_identity_arithmetic_expands_directory_inputs() {
    let dir = fresh_temp_dir("identity-arithmetic-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (x) (- x 0))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("identity-arithmetic")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
