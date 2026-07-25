use super::*;

#[test]
fn cli_flags_single_operand_plus() {
    let dir = fresh_temp_dir("single-operand-arithmetic-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (+ x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-operand-arithmetic")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"+\""));
}

#[test]
fn cli_flags_single_operand_star() {
    let dir = fresh_temp_dir("single-operand-arithmetic-report-star");
    let file = dir.join("a.lisp");
    fs::write(&file, "(* n)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-operand-arithmetic")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"*\""));
}

#[test]
fn cli_does_not_flag_unary_minus_divide_or_multi_operand() {
    let dir = fresh_temp_dir("single-operand-arithmetic-report-clean");
    let file = dir.join("a.lisp");
    // (- x) negates, (/ x) is reciprocal, (+) is 0, (+ x y) is real arithmetic.
    fs::write(&file, "(- x)\n(/ x)\n(+)\n(+ x y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-operand-arithmetic")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_single_operand_arithmetic_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("single-operand-arithmetic-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(* total)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-operand-arithmetic")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "single-operand-arithmetic-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_unwraps_single_operand_arithmetic() {
    let dir = fresh_temp_dir("single-operand-arithmetic-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (+ (compute x)))\n").expect("write a.lisp");

    // The aggregator's --fix engine should unwrap (+ (compute x)) to (compute x).
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("single-operand-arithmetic")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(defun f (x) (compute x))\n");
}
