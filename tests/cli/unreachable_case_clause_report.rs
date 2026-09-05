use super::*;

#[test]
fn cli_reports_a_clause_after_a_catch_all() {
    let dir = fresh_temp_dir("unreachable-case-clause-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x (1 :one) (t :def) (2 :two))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unreachable-case-clause")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"case_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"unreachable_count\": 1"));
}

#[test]
fn cli_does_not_flag_a_trailing_catch_all() {
    let dir = fresh_temp_dir("unreachable-case-clause-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x (1 :one) (2 :two) (t :def))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unreachable-case-clause")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "nothing stranded in this case" from
        // "no case form at all".
        .stdout(predicate::str::contains("\"case_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_does_not_flag_a_literal_t_key_list() {
    let dir = fresh_temp_dir("unreachable-case-clause-report-literal");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x ((t) :sym) (2 :two))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unreachable-case-clause")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_flags_a_typecase_clause_after_t() {
    let dir = fresh_temp_dir("unreachable-case-clause-report-typecase");
    let file = dir.join("a.lisp");
    fs::write(&file, "(typecase x (integer 1) (t 0) (string 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unreachable-case-clause")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"head\": \"typecase\""));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("unreachable-case-clause-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(case x (t 1) (2 :two))\n").expect("write a.clj");

    paredit()
        .args(["inspect", "unreachable-case-clause", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_unreachable_case_clause_emits_sarif() {
    let dir = fresh_temp_dir("unreachable-case-clause-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x (t 1) (2 :two))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "unreachable-case-clause", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/unreachable-case-clause/unreachable-case-clause\"",
        ))
        .stdout(predicate::str::contains(
            "case has 1 unreachable clause(s) after a t/otherwise catch-all",
        ));
}

#[test]
fn cli_unreachable_case_clause_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("unreachable-case-clause-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x (t 1) (2 :two))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unreachable-case-clause")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "unreachable-case-clause-report policy failed",
        ));
}

#[test]
fn cli_unreachable_case_clause_expands_directory_inputs() {
    let dir = fresh_temp_dir("unreachable-case-clause-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (x) (case x (t 1) (2 :two) (3 :three)))\n")
        .expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unreachable-case-clause")
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
