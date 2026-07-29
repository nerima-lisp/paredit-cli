use super::*;

#[test]
fn cli_reports_a_variable_assigned_to_itself() {
    let dir = fresh_temp_dir("self-assignment-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq x x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("self-assignments")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        // The envelope names the numerator `finding_count`; the denominator and
        // the per-finding fields are the ones that had to survive.
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"assignment_form_count\": 1"))
        .stdout(predicate::str::contains("\"place\": \"x\""))
        .stdout(predicate::str::contains("\"operator\": \"setq\""))
        .stdout(predicate::str::contains("\"line\": 1"));
}

#[test]
fn cli_reports_a_structural_place_nested_in_a_function_body() {
    let dir = fresh_temp_dir("self-assignment-report-nested");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (a i) (setf (aref a i) (aref a i)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("self-assignments")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("(aref a i)"));
}

#[test]
fn cli_does_not_flag_a_normal_assignment() {
    let dir = fresh_temp_dir("self-assignment-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq x y)\n(setf a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("self-assignments")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no self-assignment in two
        // assignments" from "no assignment at all".
        .stdout(predicate::str::contains("\"assignment_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("self-assignment-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(setq x x)\n").expect("write a.clj");

    paredit()
        .args(["inspect", "self-assignments", "--output", "json"])
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
fn cli_self_assignments_emits_sarif() {
    let dir = fresh_temp_dir("self-assignment-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq x x)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "self-assignments", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/self-assignments/self-assignment\"",
        ))
        .stdout(predicate::str::contains("setq assigns place x to itself"));
}

#[test]
fn cli_self_assignments_fail_on_self_assignment_trips_gate() {
    let dir = fresh_temp_dir("self-assignment-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq x x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("self-assignments")
        .arg("--fail-on-self-assignment")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "self-assignment-report policy failed",
        ));
}

#[test]
fn cli_self_assignments_passes_gate_when_clean() {
    let dir = fresh_temp_dir("self-assignment-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq x y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("self-assignments")
        .arg("--fail-on-self-assignment")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}
