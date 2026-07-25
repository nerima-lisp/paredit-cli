use super::*;

#[test]
fn cli_flags_two_arg_list_star() {
    let dir = fresh_temp_dir("list-star-to-cons-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list* a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("list-star-to-cons")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_three_args() {
    let dir = fresh_temp_dir("list-star-to-cons-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list* a b c)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("list-star-to-cons")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("list-star-to-cons-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list* a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("list-star-to-cons")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "list-star-to-cons-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_cons() {
    let dir = fresh_temp_dir("list-star-to-cons-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list* (car x) (cdr y))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("list-star-to-cons")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(cons (car x) (cdr y))\n");
}
