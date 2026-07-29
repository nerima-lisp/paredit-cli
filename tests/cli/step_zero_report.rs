use super::*;

#[test]
fn cli_flags_incf_zero() {
    let dir = fresh_temp_dir("step-zero-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(incf counter 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("step-zero")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"step_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"incf\""));
}

#[test]
fn cli_flags_decf_zero() {
    let dir = fresh_temp_dir("step-zero-report-decf");
    let file = dir.join("a.lisp");
    fs::write(&file, "(decf x 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("step-zero")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        // `decf` and `incf` are separate kinds, so a consumer can filter on
        // one direction without parsing the operator field.
        .stdout(predicate::str::contains("\"kind\": \"decf\""));
}

#[test]
fn cli_does_not_flag_nonzero_step() {
    let dir = fresh_temp_dir("step-zero-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(incf x 2)\n(decf y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("step-zero")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no zero step in two scanned
        // forms" from "no incf/decf form at all".
        .stdout(predicate::str::contains("\"step_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("step-zero-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn bump [x] (incf x 0))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "step-zero", "--output", "json"])
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
fn cli_step_zero_emits_sarif() {
    let dir = fresh_temp_dir("step-zero-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(incf counter 0)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "step-zero", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/step-zero/incf\"",
        ))
        .stdout(predicate::str::contains(
            "incf by 0 is a no-op that changes nothing",
        ));
}

#[test]
fn cli_step_zero_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("step-zero-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(incf x 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("step-zero")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("step-zero-report policy failed"));
}
