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
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"argument_count\": 3"));
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
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"setq\""));
}

#[test]
fn cli_does_not_flag_a_well_formed_setf() {
    let dir = fresh_temp_dir("setf-arity-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setf a 1 b 2)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("setf-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
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
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}
