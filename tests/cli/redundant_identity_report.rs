use super::*;

#[test]
fn cli_flags_identity_call() {
    let dir = fresh_temp_dir("redundant-identity-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun echo (x) (identity x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-identity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"identity_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"));
}

#[test]
fn cli_does_not_flag_reference_or_arity_mismatch() {
    let dir = fresh_temp_dir("redundant-identity-report-clean");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(sort xs #'< :key #'identity)\n(identity)\n(identity a b)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-identity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "two identity calls, neither of
        // them the one-argument shape" from "no identity call at all"; the
        // `#'identity` reference is not a call.
        .stdout(predicate::str::contains("\"identity_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_redundant_identity_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("redundant-identity-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(identity x)\n").expect("write a.clj");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-identity")
        .arg("--output")
        .arg("json")
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
fn cli_redundant_identity_emits_sarif() {
    let dir = fresh_temp_dir("redundant-identity-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(identity x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-identity")
        .arg("--output")
        .arg("sarif")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/redundant-identity/redundant-identity\"",
        ))
        .stdout(predicate::str::contains(
            "identity returns its argument unchanged; (identity x) is x",
        ));
}

#[test]
fn cli_redundant_identity_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-identity-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(identity result)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-identity")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-identity-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_unwraps_identity() {
    let dir = fresh_temp_dir("redundant-identity-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(identity (compute a b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-identity")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(compute a b)\n");
}

#[test]
fn cli_lint_fix_composes_with_redundant_funcall() {
    let dir = fresh_temp_dir("redundant-identity-report-fixpoint");
    let file = dir.join("a.lisp");
    // (funcall #'identity x) -> (identity x) -> x across the fixpoint.
    fs::write(&file, "(funcall #'identity x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-identity")
        .arg("--rule")
        .arg("redundant-funcall")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "x\n");
}
