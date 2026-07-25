use super::*;

#[test]
fn cli_flags_zero_minus() {
    let dir = fresh_temp_dir("verbose-negation-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun negate (x) (- 0 x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("verbose-negation")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_trailing_zero_float_or_other_constant() {
    let dir = fresh_temp_dir("verbose-negation-report-clean");
    let file = dir.join("a.lisp");
    // Trailing zero (identity), float -1.0, and a different constant.
    fs::write(&file, "(- x 0)\n(* x -1.0)\n(- 5 x)\n(* x -2)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("verbose-negation")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_verbose_negation_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("verbose-negation-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(* balance -1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("verbose-negation")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "verbose-negation-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_all_negation_shapes() {
    let dir = fresh_temp_dir("verbose-negation-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(- 0 (compute x))\n(* delta -1)\n(* -1 delta)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("verbose-negation")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(- (compute x))\n(- delta)\n(- delta)\n");
}
