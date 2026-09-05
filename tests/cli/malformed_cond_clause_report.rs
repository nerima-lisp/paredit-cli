use super::*;

#[test]
fn cli_reports_a_bare_atom_clause() {
    let dir = fresh_temp_dir("malformed-cond-clause-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((plusp x) :pos) ((minusp x) :neg) :zero)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-cond-clause")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"cond_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"clause\": \":zero\""));
}

#[test]
fn cli_does_not_flag_valid_clauses() {
    let dir = fresh_temp_dir("malformed-cond-clause-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((foo) 1) ((bar) 2) (t 3))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-cond-clause")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "every clause of one `cond` is well
        // formed" from "no `cond` form at all".
        .stdout(predicate::str::contains("\"cond_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("malformed-cond-clause-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn pick [] (cond ((foo) 1) bar))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "malformed-cond-clause", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_malformed_cond_clause_emits_sarif() {
    let dir = fresh_temp_dir("malformed-cond-clause-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((foo) 1) bar)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "malformed-cond-clause", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/malformed-cond-clause/malformed-cond-clause\"",
        ))
        .stdout(predicate::str::contains(
            "cond clause bar is not a non-empty list",
        ));
}

#[test]
fn cli_does_not_flag_a_feature_conditional_clause() {
    let dir = fresh_temp_dir("malformed-cond-clause-report-feature");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((a) 1) #+sbcl ((b) 2) (t 3))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-cond-clause")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        .stdout(predicate::str::contains("\"cond_form_count\": 1"));
}

#[test]
fn cli_malformed_cond_clause_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("malformed-cond-clause-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((foo) 1) bar)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-cond-clause")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "malformed-cond-clause-report policy failed",
        ));
}

#[test]
fn cli_malformed_cond_clause_expands_directory_inputs() {
    let dir = fresh_temp_dir("malformed-cond-clause-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (x) (cond ((p x) 1) x))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-cond-clause")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
