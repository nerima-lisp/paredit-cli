use super::*;

#[test]
fn cli_flags_manual_pushnew() {
    let dir = fresh_temp_dir("manual-pushnew-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun note (k) (setf keys (adjoin k keys)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("manual-pushnew")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_adjoin_element_or_other_variable() {
    let dir = fresh_temp_dir("manual-pushnew-report-clean");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(setf xs (adjoin xs other))\n(setf xs (adjoin item ys))\n(setf (slot o) (adjoin item (slot o)))\n(setf xs (cons item xs))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("manual-pushnew")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_manual_pushnew_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("manual-pushnew-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setf seen (adjoin k seen))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("manual-pushnew")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "manual-pushnew-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_manual_pushnew_passing_keywords_through() {
    let dir = fresh_temp_dir("manual-pushnew-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setf seen (adjoin k seen :test #'equal))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("manual-pushnew")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(pushnew k seen :test #'equal)\n");
}
