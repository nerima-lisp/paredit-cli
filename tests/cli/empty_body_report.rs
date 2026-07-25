use super::*;

#[test]
fn cli_flags_when_with_no_body() {
    let dir = fresh_temp_dir("empty-body-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (ready) (when ready))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("empty-body")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"head\": \"when\""));
}

#[test]
fn cli_flags_dolist_with_no_body() {
    let dir = fresh_temp_dir("empty-body-report-dolist");
    let file = dir.join("a.lisp");
    fs::write(&file, "(dolist (x items))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("empty-body")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"head\": \"dolist\""));
}

#[test]
fn cli_does_not_flag_forms_with_a_body() {
    let dir = fresh_temp_dir("empty-body-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when ready (go))\n(dolist (x items) (print x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("empty-body")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_empty_body_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("empty-body-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(unless done)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("empty-body")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("empty-body-report policy failed"));
}

#[test]
fn cli_empty_body_expands_directory_inputs() {
    let dir = fresh_temp_dir("empty-body-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (n) (dotimes (i n)))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("empty-body")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}
