use super::*;

#[test]
fn cli_reports_a_nil_and_keyword_binding() {
    let dir = fresh_temp_dir("binds-constant-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let ((nil 1) (:status :ok)) :status)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("binds-constant")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 2"))
        .stdout(predicate::str::contains("\"nil\""))
        .stdout(predicate::str::contains("\":status\""));
}

#[test]
fn cli_does_not_flag_ordinary_bindings() {
    let dir = fresh_temp_dir("binds-constant-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let ((x 1) (*g* 2) (pkg:sym 3)) x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("binds-constant")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_flags_a_do_binding_of_t() {
    let dir = fresh_temp_dir("binds-constant-report-do");
    let file = dir.join("a.lisp");
    fs::write(&file, "(do ((t 0 (1+ t))) ((done)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("binds-constant")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"do\""));
}

#[test]
fn cli_binds_constant_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("binds-constant-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let ((t 1)) t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("binds-constant")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "binds-constant-report policy failed",
        ));
}

#[test]
fn cli_binds_constant_expands_directory_inputs() {
    let dir = fresh_temp_dir("binds-constant-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f () (let ((nil 1)) nil))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("binds-constant")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}
