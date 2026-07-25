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
        .stdout(predicate::str::contains("\"violation_count\": 1"))
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
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_nonzero_step() {
    let dir = fresh_temp_dir("step-zero-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(incf x 2)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("step-zero")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
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
