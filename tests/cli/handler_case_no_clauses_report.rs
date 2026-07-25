use super::*;

#[test]
fn cli_flags_clauseless() {
    let dir = fresh_temp_dir("handler-case-no-clauses-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(handler-case (compute))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("handler-case-no-clauses")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_with_clauses() {
    let dir = fresh_temp_dir("handler-case-no-clauses-report-clauses");
    let file = dir.join("a.lisp");
    fs::write(&file, "(handler-case (compute) (error (e) nil))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("handler-case-no-clauses")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_handler_case_no_clauses_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("handler-case-no-clauses-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(handler-case x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("handler-case-no-clauses")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "handler-case-no-clauses-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_unwraps() {
    let dir = fresh_temp_dir("handler-case-no-clauses-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(handler-case (compute))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("handler-case-no-clauses")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(compute)\n");
}
