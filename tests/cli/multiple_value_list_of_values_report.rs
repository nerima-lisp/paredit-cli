use super::*;

#[test]
fn cli_flags_mvl_of_values() {
    let dir = fresh_temp_dir("multiple-value-list-of-values-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(multiple-value-list (values a b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("multiple-value-list-of-values")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_variable_arg() {
    let dir = fresh_temp_dir("multiple-value-list-of-values-report-variable");
    let file = dir.join("a.lisp");
    fs::write(&file, "(multiple-value-list (compute))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("multiple-value-list-of-values")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("multiple-value-list-of-values-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(multiple-value-list (values a b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("multiple-value-list-of-values")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "multiple-value-list-of-values-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_list() {
    let dir = fresh_temp_dir("multiple-value-list-of-values-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(multiple-value-list (values (car x) (cdr x)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("multiple-value-list-of-values")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(list (car x) (cdr x))\n");
}
