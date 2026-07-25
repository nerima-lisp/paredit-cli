use super::*;

#[test]
fn cli_flags_if_x_x_y() {
    let dir = fresh_temp_dir("if-to-or-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if cached cached (compute))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-to-or")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_compound_or_constant_test() {
    let dir = fresh_temp_dir("if-to-or-report-clean");
    let file = dir.join("a.lisp");
    // A differing then, a compound (double-eval) test, and literal t/nil tests
    // are all left alone.
    fs::write(
        &file,
        "(if a b c)\n(if (pop s) (pop s) y)\n(if t t y)\n(if nil nil y)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-to-or")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_if_to_or_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("if-to-or-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if found found (search))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-to-or")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("if-to-or-report policy failed"));
}

#[test]
fn cli_lint_fix_rewrites_as_or() {
    let dir = fresh_temp_dir("if-to-or-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if cached cached (compute x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("if-to-or")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(or cached (compute x))\n");
}
