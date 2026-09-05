use super::*;

#[test]
fn cli_reports_a_parameter_named_twice() {
    let dir = fresh_temp_dir("duplicate-parameter-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun scale (factor x factor) (* factor x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-parameters")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"definition_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"scale\""))
        .stdout(predicate::str::contains("\"factor\""))
        .stdout(predicate::str::contains("\"occurrence_count\": 2"));
}

#[test]
fn cli_does_not_flag_distinct_parameters() {
    let dir = fresh_temp_dir("duplicate-parameter-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x y &optional z) (+ x y z))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-parameters")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no repeated parameter in one
        // definition" from "no callable definition at all".
        .stdout(predicate::str::contains("\"definition_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("duplicate-parameter-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(defun f (x x) x)\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "duplicate-parameters", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_duplicate_parameters_emits_sarif() {
    let dir = fresh_temp_dir("duplicate-parameter-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x x) x)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "duplicate-parameters", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/duplicate-parameters/duplicate-parameters\"",
        ))
        .stdout(predicate::str::contains(
            "f names parameter x more than once",
        ));
}

#[test]
fn cli_duplicate_parameters_fail_on_duplicate_trips_gate() {
    let dir = fresh_temp_dir("duplicate-parameter-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x x) x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-parameters")
        .arg("--fail-on-duplicate")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "duplicate-parameter-report policy failed",
        ));
}

#[test]
fn cli_duplicate_parameters_passes_gate_when_clean() {
    let dir = fresh_temp_dir("duplicate-parameter-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (a b c) (list a b c))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-parameters")
        .arg("--fail-on-duplicate")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_duplicate_parameters_expands_directory_inputs() {
    let dir = fresh_temp_dir("duplicate-parameter-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defmacro m (a a) a)\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-parameters")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"m\""));
}
