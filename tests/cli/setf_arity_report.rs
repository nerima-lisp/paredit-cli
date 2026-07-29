use super::*;

#[test]
fn cli_reports_an_odd_arity_setf() {
    let dir = fresh_temp_dir("setf-arity-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setf (slot a) 1 (slot b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("setf-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"assignment_form_count\": 1"))
        .stdout(predicate::str::contains("\"argument_count\": 3"))
        .stdout(predicate::str::contains("\"line\": 1"));
}

#[test]
fn cli_reports_an_odd_arity_setq() {
    let dir = fresh_temp_dir("setf-arity-report-setq");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq x 1 y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("setf-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"setq\""));
}

#[test]
fn cli_does_not_flag_a_well_formed_setf() {
    let dir = fresh_temp_dir("setf-arity-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setf a 1 b 2)\n(setq c 3)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("setf-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no odd arity in two assignments"
        // from "no assignment at all".
        .stdout(predicate::str::contains("\"assignment_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("setf-arity-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(setf a 1 b)\n").expect("write a.clj");

    paredit()
        .args(["inspect", "setf-arity", "--output", "json"])
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
fn cli_setf_arity_emits_sarif() {
    let dir = fresh_temp_dir("setf-arity-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setf a 1 b)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "setf-arity", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/setf-arity/setf-arity\"",
        ))
        .stdout(predicate::str::contains(
            "setf has 3 arguments; place/value pairs require an even count",
        ));
}

#[test]
fn cli_setf_arity_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("setf-arity-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setf a 1 b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("setf-arity")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("setf-arity-report policy failed"));
}

#[test]
fn cli_setf_arity_passes_gate_when_clean() {
    let dir = fresh_temp_dir("setf-arity-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setf a 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("setf-arity")
        .arg("--fail-on-violation")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}
