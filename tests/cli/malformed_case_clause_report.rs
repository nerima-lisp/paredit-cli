use super::*;

#[test]
fn cli_reports_dropped_paren_clauses() {
    let dir = fresh_temp_dir("malformed-case-clause-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x (1 :one) 2 :two)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-case-clause")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 2"))
        .stdout(predicate::str::contains("\"case_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"head\": \"case\""))
        .stdout(predicate::str::contains("\"clause\": \"2\""))
        .stdout(predicate::str::contains("\"clause\": \":two\""));
}

#[test]
fn cli_does_not_flag_valid_clauses() {
    let dir = fresh_temp_dir("malformed-case-clause-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x (1 :one) ((2 3) :multi) (otherwise :o))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-case-clause")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "every clause of one `case` is well
        // formed" from "no `case` form at all".
        .stdout(predicate::str::contains("\"case_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("malformed-case-clause-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn pick [x] (case x (1 :one) foo))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "malformed-case-clause", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_malformed_case_clause_emits_sarif() {
    let dir = fresh_temp_dir("malformed-case-clause-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x (1 :one) oops)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "malformed-case-clause", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/malformed-case-clause/malformed-case-clause\"",
        ))
        .stdout(predicate::str::contains(
            "case clause oops is not a non-empty list",
        ));
}

#[test]
fn cli_does_not_flag_a_feature_conditional_clause() {
    let dir = fresh_temp_dir("malformed-case-clause-report-feature");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x (1 :one) #+sbcl (2 :two) (t :o))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-case-clause")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        .stdout(predicate::str::contains("\"case_form_count\": 1"));
}

#[test]
fn cli_flags_a_malformed_typecase_clause() {
    let dir = fresh_temp_dir("malformed-case-clause-report-typecase");
    let file = dir.join("a.lisp");
    fs::write(&file, "(typecase y (integer 1) bogus (t 0))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-case-clause")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        // The head keeps its own field rather than becoming the `kind`, so a
        // consumer can still tell a `typecase` clause from a `case` one.
        .stdout(predicate::str::contains("\"head\": \"typecase\""));
}

#[test]
fn cli_malformed_case_clause_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("malformed-case-clause-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(ecase x (:a 1) bad)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-case-clause")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "malformed-case-clause-report policy failed",
        ));
}

#[test]
fn cli_malformed_case_clause_expands_directory_inputs() {
    let dir = fresh_temp_dir("malformed-case-clause-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (x) (case x (1 :one) oops))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-case-clause")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
