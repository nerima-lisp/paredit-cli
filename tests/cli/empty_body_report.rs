use super::*;

#[test]
fn cli_flags_when_with_no_body() {
    let dir = fresh_temp_dir("empty-body-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (ready) (when ready))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("empty-body")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"body_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"head\": \"when\""));
}

#[test]
fn cli_flags_dolist_with_no_body() {
    let dir = fresh_temp_dir("empty-body-report-dolist");
    let file = dir.join("a.lisp");
    fs::write(&file, "(dolist (x items))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("empty-body")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"head\": \"dolist\""));
}

#[test]
fn cli_does_not_flag_forms_with_a_body() {
    let dir = fresh_temp_dir("empty-body-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when ready (go))\n(dolist (x items) (print x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("empty-body")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "both body-taking forms have a
        // body" from "no body-taking form at all".
        .stdout(predicate::str::contains("\"body_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("empty-body-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [ready] (when ready))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "empty-body", "--output", "json"])
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
fn cli_empty_body_emits_sarif() {
    let dir = fresh_temp_dir("empty-body-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(unless done)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "empty-body", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/empty-body/empty-body\"",
        ))
        .stdout(predicate::str::contains(
            "unless has no body; the test/spec runs, then nothing",
        ));
}

#[test]
fn cli_empty_body_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("empty-body-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(unless done)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("empty-body")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("empty-body-report policy failed"));
}

#[test]
fn cli_empty_body_expands_directory_inputs() {
    let dir = fresh_temp_dir("empty-body-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (n) (dotimes (i n)))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("empty-body")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
