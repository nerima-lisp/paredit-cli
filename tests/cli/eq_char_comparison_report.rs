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
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"comparison_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
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
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
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
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no character argument in one `eq`"
        // from "no `eq` at all".
        .stdout(predicate::str::contains("\"comparison_form_count\": 1"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("eq-char-comparison-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn pick [c] (= c 97))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "eq-char-comparison", "--output", "json"])
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
fn cli_eq_char_comparison_emits_sarif() {
    let dir = fresh_temp_dir("eq-char-comparison-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq c #\\a)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "eq-char-comparison", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/eq-char-comparison/eq-char-comparison\"",
        ))
        .stdout(predicate::str::contains("use eql or char="));
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
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
