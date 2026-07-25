use super::*;

#[test]
fn cli_flags_single_variable_bind() {
    let dir = fresh_temp_dir("single-value-bind-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(multiple-value-bind (q) (truncate a b) (use q))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-value-bind")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_multi_variable_or_empty_bind() {
    let dir = fresh_temp_dir("single-value-bind-report-clean");
    let file = dir.join("a.lisp");
    // Two variables capture secondary values; an empty list is a progn.
    fs::write(
        &file,
        "(multiple-value-bind (q r) (truncate a b) q)\n(multiple-value-bind () (f) done)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-value-bind")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_single_value_bind_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("single-value-bind-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(multiple-value-bind (v) (compute) (list v))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-value-bind")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "single-value-bind-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_bind_as_let() {
    let dir = fresh_temp_dir("single-value-bind-report-fix");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(multiple-value-bind (q) (truncate a b) (use q) (log q))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("single-value-bind")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(let ((q (truncate a b))) (use q) (log q))\n");
}
