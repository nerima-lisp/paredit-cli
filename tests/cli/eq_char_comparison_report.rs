use super::*;

#[test]
fn cli_flags_eq_against_a_character_literal() {
    let dir = fresh_temp_dir("eq-char-comparison-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (c) (when (eq c #\\a) :hit))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eq-char-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        // The literal `#\a` is JSON-escaped to `#\\a` in the output.
        .stdout(predicate::str::contains("#\\\\a"));
}

#[test]
fn cli_does_not_flag_eql_or_char_equal() {
    let dir = fresh_temp_dir("eq-char-comparison-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eql c #\\a)\n(char= c #\\a)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eq-char-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_does_not_flag_eq_against_a_number() {
    let dir = fresh_temp_dir("eq-char-comparison-report-number");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq n 5)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eq-char-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_eq_char_comparison_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("eq-char-comparison-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq c #\\Space)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eq-char-comparison")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "eq-char-comparison-report policy failed",
        ));
}

#[test]
fn cli_eq_char_comparison_expands_directory_inputs() {
    let dir = fresh_temp_dir("eq-char-comparison-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (c) (eq c #\\z))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("eq-char-comparison")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}
