use super::*;

#[test]
fn cli_reports_a_clause_after_a_catch_all() {
    let dir = fresh_temp_dir("unreachable-cond-clause-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((foo) 1) (t 2) ((bar) 3))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unreachable-cond-clause")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"cond_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"unreachable_count\": 1"));
}

#[test]
fn cli_does_not_flag_a_trailing_catch_all() {
    let dir = fresh_temp_dir("unreachable-cond-clause-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((foo) 1) ((bar) 2) (t 3))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unreachable-cond-clause")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "nothing stranded in this cond" from
        // "no cond form at all".
        .stdout(predicate::str::contains("\"cond_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("unreachable-cond-clause-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(cond ((foo) 1) (t 2) ((bar) 3))\n").expect("write a.clj");

    paredit()
        .args(["inspect", "unreachable-cond-clause", "--output", "json"])
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
fn cli_unreachable_cond_clause_emits_sarif() {
    let dir = fresh_temp_dir("unreachable-cond-clause-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond (t 1) ((a) 2))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "unreachable-cond-clause", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/unreachable-cond-clause/unreachable-cond-clause\"",
        ))
        .stdout(predicate::str::contains(
            "cond has 1 unreachable clause(s) after a t clause",
        ));
}

#[test]
fn cli_unreachable_cond_clause_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("unreachable-cond-clause-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond (t 1) ((a) 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unreachable-cond-clause")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "unreachable-cond-clause-report policy failed",
        ));
}

#[test]
fn cli_unreachable_cond_clause_passes_gate_when_clean() {
    let dir = fresh_temp_dir("unreachable-cond-clause-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((a) 1) ((b) 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unreachable-cond-clause")
        .arg("--fail-on-violation")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_unreachable_cond_clause_expands_directory_inputs() {
    let dir = fresh_temp_dir("unreachable-cond-clause-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(cond (t 1) ((a) 2) ((b) 3))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unreachable-cond-clause")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"unreachable_count\": 2"));
}
