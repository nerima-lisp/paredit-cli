use super::*;

#[test]
fn cli_flags_nested_car_cdr() {
    let dir = fresh_temp_dir("nested-cxr-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun second-of (pair) (car (cdr pair)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-cxr")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"combined\": \"cadr\""));
}

#[test]
fn cli_does_not_flag_single_accessor_or_non_accessor() {
    let dir = fresh_temp_dir("nested-cxr-report-clean");
    let file = dir.join("a.lisp");
    // Single accessor, car of a non-accessor, first/rest spelling, over-long combo.
    fs::write(
        &file,
        "(car x)\n(car (reverse x))\n(first (rest x))\n(caddr (caddr x))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-cxr")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_nested_cxr_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("nested-cxr-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cdr (cdr items))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-cxr")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("nested-cxr-report policy failed"));
}

#[test]
fn cli_lint_fix_collapses_one_level() {
    let dir = fresh_temp_dir("nested-cxr-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (cdr (lookup k)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("nested-cxr")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(cadr (lookup k))\n");
}

#[test]
fn cli_lint_fix_converges_deep_nesting_to_caddr() {
    let dir = fresh_temp_dir("nested-cxr-report-fixpoint");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (cdr (cdr x)))\n").expect("write a.lisp");

    // The fixpoint loop peels one accessor per pass: cadr, then caddr.
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("nested-cxr")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(caddr x)\n");
}
