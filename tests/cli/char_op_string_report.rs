use super::*;

#[test]
fn cli_flags_char_equal_with_a_string() {
    let dir = fresh_temp_dir("char-op-string-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (c) (when (char= c \"a\") :hit))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("char-op-string")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"char=\""));
}

#[test]
fn cli_flags_char_code_of_a_string() {
    let dir = fresh_temp_dir("char-op-string-report-code");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char-code \"x\")\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("char-op-string")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"char-code\""));
}

#[test]
fn cli_does_not_flag_char_literals_or_string_functions() {
    let dir = fresh_temp_dir("char-op-string-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char= c #\\a)\n(string= \"a\" b)\n(char= x y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("char-op-string")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_char_op_string_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("char-op-string-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char-upcase \"z\")\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("char-op-string")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "char-op-string-report policy failed",
        ));
}

#[test]
fn cli_char_op_string_expands_directory_inputs() {
    let dir = fresh_temp_dir("char-op-string-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (c) (alpha-char-p \"a\"))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("char-op-string")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}
