use super::*;

#[test]
fn cli_flags_values_list_of_list() {
    let dir = fresh_temp_dir("values-list-of-list-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(values-list (list a b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("values-list-of-list")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_quoted_list() {
    let dir = fresh_temp_dir("values-list-of-list-report-quoted");
    let file = dir.join("a.lisp");
    fs::write(&file, "(values-list '(a b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("values-list-of-list")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("values-list-of-list-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(values-list (list a b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("values-list-of-list")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "values-list-of-list-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_values() {
    let dir = fresh_temp_dir("values-list-of-list-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(values-list (list (car x) (cdr x)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("values-list-of-list")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(values (car x) (cdr x))\n");
}
