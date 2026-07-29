use super::*;

#[test]
fn cli_reports_a_dropped_paren_binding() {
    let dir = fresh_temp_dir("malformed-let-binding-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let ((x 1 y 2)) (+ x y))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-let-binding")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"let_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"element_count\": 4"))
        .stdout(predicate::str::contains("\"binding\":"));
}

#[test]
fn cli_does_not_flag_valid_bindings() {
    let dir = fresh_temp_dir("malformed-let-binding-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let ((x 1) (y) z) (list x y z))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-let-binding")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "every binding of the one `let`
        // here is well-formed" from "there is no `let` at all".
        .stdout(predicate::str::contains("\"let_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("malformed-let-binding-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [] (let ((x 1 2)) x))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "malformed-let-binding", "--output", "json"])
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
fn cli_malformed_let_binding_emits_sarif() {
    let dir = fresh_temp_dir("malformed-let-binding-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let ((x 1 2)) x)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "malformed-let-binding", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/malformed-let-binding/malformed-let-binding\"",
        ))
        .stdout(predicate::str::contains(
            "elements; expected a symbol or (var value)",
        ));
}

#[test]
fn cli_malformed_let_binding_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("malformed-let-binding-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let ((x 1 2)) x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-let-binding")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "malformed-let-binding-report policy failed",
        ));
}

#[test]
fn cli_malformed_let_binding_passes_gate_when_clean() {
    let dir = fresh_temp_dir("malformed-let-binding-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let* ((x 1) (y (* x 2))) y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-let-binding")
        .arg("--fail-on-violation")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_malformed_let_binding_expands_directory_inputs() {
    let dir = fresh_temp_dir("malformed-let-binding-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(let* ((x 1 2)) x)\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-let-binding")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"element_count\": 3"));
}
