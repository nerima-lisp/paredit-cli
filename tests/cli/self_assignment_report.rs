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
        .stdout(predicate::str::contains("\"self_assignment_count\": 1"))
        .stdout(predicate::str::contains("\"place\": \"x\""));
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
        .stdout(predicate::str::contains("\"self_assignment_count\": 1"))
        .stdout(predicate::str::contains("(aref a i)"));
}

#[test]
fn cli_does_not_flag_a_normal_assignment() {
    let dir = fresh_temp_dir("self-assignment-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(setq x y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("self-assignments")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"self_assignment_count\": 0"));
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
        .stdout(predicate::str::contains("\"self_assignment_count\": 0"));
}
