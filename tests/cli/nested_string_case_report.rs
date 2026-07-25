use super::*;

#[test]
fn cli_flags_nested() {
    let dir = fresh_temp_dir("nested-string-case-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(string-upcase (string-downcase s))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-string-case")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_single() {
    let dir = fresh_temp_dir("nested-string-case-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(string-upcase s)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-string-case")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("nested-string-case-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(string-upcase (string-downcase s))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-string-case")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "nested-string-case-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_collapses() {
    let dir = fresh_temp_dir("nested-string-case-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(string-downcase (string-capitalize name))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("nested-string-case")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(string-downcase name)\n");
}
