use super::*;

#[test]
fn cli_flags_nested_char_case() {
    let dir = fresh_temp_dir("nested-char-case-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char-upcase (char-downcase c))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-char-case")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_single_case() {
    let dir = fresh_temp_dir("nested-char-case-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char-upcase c)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-char-case")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("nested-char-case-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char-upcase (char-downcase c))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-char-case")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "nested-char-case-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_collapses_to_outer() {
    let dir = fresh_temp_dir("nested-char-case-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char-upcase (char-downcase c))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("nested-char-case")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(char-upcase c)\n");
}
