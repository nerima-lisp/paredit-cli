use super::*;

#[test]
fn cli_flags_if_nil_t() {
    let dir = fresh_temp_dir("if-not-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if ready nil t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-not")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"if_form_count\": 1"));
}

#[test]
fn cli_does_not_flag_boolean_coercion() {
    let dir = fresh_temp_dir("if-not-report-clean");
    let file = dir.join("a.lisp");
    // (if test t nil) is a coercion with no clearer single builtin.
    fs::write(&file, "(if x t nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-not")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_if_not_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("if-not-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if x nil t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-not")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("if-not-report policy failed"));
}

#[test]
fn cli_lint_fix_rewrites_to_not() {
    let dir = fresh_temp_dir("if-not-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if (ready-p x) nil t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("if-not")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(not (ready-p x))\n");
}
