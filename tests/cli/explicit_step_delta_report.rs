use super::*;

#[test]
fn cli_flags_incf_explicit_one() {
    let dir = fresh_temp_dir("explicit-step-delta-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(incf counter 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("explicit-step-delta")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"incf\""));
}

#[test]
fn cli_does_not_flag_non_unit_float_or_implicit_delta() {
    let dir = fresh_temp_dir("explicit-step-delta-report-clean");
    let file = dir.join("a.lisp");
    // A non-1 delta, a float 1.0 (type-coercing), and the implicit default are
    // all left alone.
    fs::write(&file, "(incf x 2)\n(incf y 1.0)\n(decf z)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("explicit-step-delta")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_explicit_step_delta_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("explicit-step-delta-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(decf remaining 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("explicit-step-delta")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "explicit-step-delta-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_the_default_delta() {
    let dir = fresh_temp_dir("explicit-step-delta-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(incf (aref v i) 1)\n(decf n 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("explicit-step-delta")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(incf (aref v i))\n(decf n)\n");
}
