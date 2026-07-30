use super::*;

#[test]
fn cli_flags_t_in_and() {
    let dir = fresh_temp_dir("redundant-boolean-identity-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun ok? (a b) (and a t b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-boolean-identity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"boolean_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"kind\": \"and\""))
        .stdout(predicate::str::contains("\"operator\": \"and\""))
        .stdout(predicate::str::contains("\"identity\": \"t\""));
}

#[test]
fn cli_does_not_flag_trailing_t_or_dominant_elements() {
    let dir = fresh_temp_dir("redundant-boolean-identity-report-clean");
    let file = dir.join("a.lisp");
    // Trailing t (return value), dominant nil in and, dominant t in or, single operand.
    fs::write(&file, "(and a t)\n(and a nil b)\n(or a t b)\n(and t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-boolean-identity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no redundant identity among four
        // boolean forms" from "no boolean form at all".
        .stdout(predicate::str::contains("\"boolean_form_count\": 4"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("redundant-boolean-identity-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(and a t b)\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "redundant-boolean-identity", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

/// The envelope's interchange formats, which this report reached by moving onto
/// it. Asserted here only far enough to prove the command accepts them; their
/// content is covered once in `report_interop`. The `or` in the rule id is the
/// finding's `kind`, which this report gets from its operator.
#[test]
fn cli_redundant_boolean_identity_emits_sarif() {
    let dir = fresh_temp_dir("redundant-boolean-identity-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(or found nil next)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "redundant-boolean-identity", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/redundant-boolean-identity/or\"",
        ))
        .stdout(predicate::str::contains(
            "or has a redundant nil operand; drop it",
        ));
}

#[test]
fn cli_redundant_boolean_identity_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-boolean-identity-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(or found nil next)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-boolean-identity")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-boolean-identity-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_identity_operands() {
    let dir = fresh_temp_dir("redundant-boolean-identity-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(and a t b)\n(or a nil b)\n(or nil nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-boolean-identity")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(and a b)\n(or a b)\nnil\n");
}

#[test]
fn cli_lint_fix_composes_with_single_operand_boolean() {
    let dir = fresh_temp_dir("redundant-boolean-identity-report-fixpoint");
    let file = dir.join("a.lisp");
    // (and t x) -> (and x) -> x across the fixpoint.
    fs::write(&file, "(and t x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-boolean-identity")
        .arg("--rule")
        .arg("single-operand-boolean")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "x\n");
}
