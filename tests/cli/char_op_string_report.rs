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
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"char_call_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"char=\""))
        .stdout(predicate::str::contains("\"literal\": \"\\\"a\\\"\""));
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
        .stdout(predicate::str::contains("\"finding_count\": 1"))
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
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "two character calls, both given a
        // character" from "no character call at all"; `string=` is not one.
        .stdout(predicate::str::contains("\"char_call_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("char-op-string-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [c] (= c \"a\"))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "char-op-string", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

/// The envelope's interchange formats, which this report reached by moving onto
/// it. Asserted here only far enough to prove the command accepts them; their
/// content is covered once in `report_interop`.
#[test]
fn cli_char_op_string_emits_sarif() {
    let dir = fresh_temp_dir("char-op-string-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char-code \"x\")\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "char-op-string", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/char-op-string/string-literal\"",
        ))
        .stdout(predicate::str::contains(
            "char-code is given string literal \\\"x\\\"; it requires a character (type error)",
        ));
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
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
