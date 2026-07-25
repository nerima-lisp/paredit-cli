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
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"element_count\": 4"));
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
        .stdout(predicate::str::contains("\"violation_count\": 0"));
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
        .stdout(predicate::str::contains("\"violation_count\": 0"));
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
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"element_count\": 3"));
}
