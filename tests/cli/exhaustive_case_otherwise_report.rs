use super::*;

#[test]
fn cli_reports_a_t_clause_in_ecase() {
    let dir = fresh_temp_dir("exhaustive-case-otherwise-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(ecase x (1 :one) (t :default))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("exhaustive-case-otherwise")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"case_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"ecase\""))
        .stdout(predicate::str::contains("\"designator\": \"t\""));
}

#[test]
fn cli_reports_an_otherwise_clause_in_etypecase() {
    let dir = fresh_temp_dir("exhaustive-case-otherwise-report-etypecase");
    let file = dir.join("a.lisp");
    fs::write(&file, "(etypecase x (integer 1) (otherwise 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("exhaustive-case-otherwise")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"etypecase\""));
}

#[test]
fn cli_does_not_flag_case_default_or_normal_ecase() {
    let dir = fresh_temp_dir("exhaustive-case-otherwise-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x (1 :a) (t :b))\n(ecase y (1 :a) (2 :b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("exhaustive-case-otherwise")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "the one exhaustive form here has
        // no default" from "no exhaustive form at all"; `case` is not counted.
        .stdout(predicate::str::contains("\"case_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_does_not_flag_a_literal_t_key_list() {
    let dir = fresh_temp_dir("exhaustive-case-otherwise-report-literal");
    let file = dir.join("a.lisp");
    fs::write(&file, "(ecase x ((t) :sym) (1 :a))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("exhaustive-case-otherwise")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("exhaustive-case-otherwise-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [x] (ecase x (1 :a) (t :b)))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "exhaustive-case-otherwise", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_exhaustive_case_otherwise_emits_sarif() {
    let dir = fresh_temp_dir("exhaustive-case-otherwise-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(ecase x (1 :one) (t :default))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "exhaustive-case-otherwise", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/exhaustive-case-otherwise/exhaustive-case-otherwise\"",
        ))
        .stdout(predicate::str::contains(
            "ecase does not permit a t clause (it is exhaustive)",
        ));
}

#[test]
fn cli_exhaustive_case_otherwise_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("exhaustive-case-otherwise-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(ccase x (1 :a) (otherwise :b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("exhaustive-case-otherwise")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "exhaustive-case-otherwise-report policy failed",
        ));
}

#[test]
fn cli_exhaustive_case_otherwise_expands_directory_inputs() {
    let dir = fresh_temp_dir("exhaustive-case-otherwise-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (x) (ecase x (1 :a) (t :b)))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("exhaustive-case-otherwise")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
