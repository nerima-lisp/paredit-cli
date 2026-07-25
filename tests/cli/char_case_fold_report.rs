use super::*;

#[test]
fn cli_flags_downcase_both() {
    let dir = fresh_temp_dir("char-case-fold-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char= (char-downcase a) (char-downcase b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("char-case-fold")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_mixed() {
    let dir = fresh_temp_dir("char-case-fold-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char= (char-downcase a) (char-upcase b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("char-case-fold")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("char-case-fold-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char= (char-downcase a) (char-downcase b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("char-case-fold")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "char-case-fold-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_char_equal() {
    let dir = fresh_temp_dir("char-case-fold-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char= (char-downcase a) (char-downcase b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("char-case-fold")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(char-equal a b)\n");
}
