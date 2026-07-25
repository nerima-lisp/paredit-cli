use super::*;

#[test]
fn cli_flags_incf_negative() {
    let dir = fresh_temp_dir("negated-step-delta-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(incf counter -1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-step-delta")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"opposite\": \"decf\""));
}

#[test]
fn cli_does_not_flag_positive_or_variable_delta() {
    let dir = fresh_temp_dir("negated-step-delta-report-clean");
    let file = dir.join("a.lisp");
    // A positive literal, the bare `-` symbol, and a variable delta are all fine.
    fs::write(&file, "(incf x 1)\n(incf x -)\n(incf x step)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-step-delta")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_negated_step_delta_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("negated-step-delta-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(decf remaining -5)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-step-delta")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "negated-step-delta-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_flips_the_operator() {
    let dir = fresh_temp_dir("negated-step-delta-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(incf (aref v i) -5)\n(decf n -2)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("negated-step-delta")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(decf (aref v i) 5)\n(incf n 2)\n");
}

#[test]
fn cli_lint_composes_with_explicit_step_delta() {
    // (incf x -1) -> (decf x 1) -> (decf x) via the fixpoint loop.
    let dir = fresh_temp_dir("negated-step-delta-report-compose");
    let file = dir.join("a.lisp");
    fs::write(&file, "(incf tally -1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("negated-step-delta")
        .arg("--rule")
        .arg("explicit-step-delta")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(decf tally)\n");
}
