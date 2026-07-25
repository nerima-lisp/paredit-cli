use super::*;

#[test]
fn cli_flags_a_progn_body_of_when() {
    let dir = fresh_temp_dir("redundant-body-progn-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when ready (progn (log) (run)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-body-progn")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"parent\": \"when\""));
}

#[test]
fn cli_flags_a_progn_body_of_defun() {
    let dir = fresh_temp_dir("redundant-body-progn-report-defun");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (progn (a) (b)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-body-progn")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"parent\": \"defun\""));
}

#[test]
fn cli_does_not_flag_single_form_or_binding_init_progns() {
    let dir = fresh_temp_dir("redundant-body-progn-report-clean");
    let file = dir.join("a.lisp");
    // Single-form progn (redundant-progn's job) and a progn as a binding init.
    fs::write(&file, "(when c (progn x))\n(let ((y (progn a b))) y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-body-progn")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_redundant_body_progn_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-body-progn-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(unless done (progn a b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-body-progn")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-body-progn-report policy failed",
        ));
}

#[test]
fn cli_redundant_body_progn_expands_directory_inputs() {
    let dir = fresh_temp_dir("redundant-body-progn-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun g () (let ((x 1)) (progn (p) (q))))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-body-progn")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}
