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
        .stdout(predicate::str::contains("\"violation_count\": 1"));
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
        .stdout(predicate::str::contains("\"violation_count\": 0"));
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
