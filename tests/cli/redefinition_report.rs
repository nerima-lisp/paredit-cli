use super::*;

#[test]
fn cli_reports_the_same_function_defined_in_two_files() {
    let dir = fresh_temp_dir("redefinition-report");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    fs::write(&a_file, "(defun helper () 1)\n").expect("write a.lisp");
    fs::write(&b_file, "(defun helper () 2)\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redefinitions")
        .arg("--output")
        .arg("json")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"redefinition_count\": 1"))
        .stdout(predicate::str::contains("\"helper\""));
}

#[test]
fn cli_does_not_flag_the_same_name_in_different_packages() {
    let dir = fresh_temp_dir("redefinition-report-packages");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    fs::write(&a_file, "(in-package :a)\n(defun helper () 1)\n").expect("write a.lisp");
    fs::write(&b_file, "(in-package :b)\n(defun helper () 2)\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redefinitions")
        .arg("--output")
        .arg("json")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"redefinition_count\": 0"));
}

#[test]
fn cli_redefinitions_fail_on_redefinition_trips_gate() {
    let dir = fresh_temp_dir("redefinition-report-gate");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    fs::write(&a_file, "(defun helper () 1)\n").expect("write a.lisp");
    fs::write(&b_file, "(defun helper () 2)\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redefinitions")
        .arg("--fail-on-redefinition")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redefinition-report policy failed",
        ));
}

#[test]
fn cli_redefinitions_passes_gate_when_all_names_are_distinct() {
    let dir = fresh_temp_dir("redefinition-report-gate-clean");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    fs::write(&a_file, "(defun helper-a () 1)\n").expect("write a.lisp");
    fs::write(&b_file, "(defun helper-b () 2)\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redefinitions")
        .arg("--fail-on-redefinition")
        .arg("--output")
        .arg("json")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"redefinition_count\": 0"));
}
