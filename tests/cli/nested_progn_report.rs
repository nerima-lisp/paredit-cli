use super::*;

#[test]
fn cli_flags_a_nested_progn() {
    let dir = fresh_temp_dir("nested-progn-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(progn (setup) (progn (a) (b)) (teardown))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-progn")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"body_form_count\": 2"));
}

#[test]
fn cli_flags_multiple_nested_progns() {
    let dir = fresh_temp_dir("nested-progn-report-many");
    let file = dir.join("a.lisp");
    fs::write(&file, "(progn (progn a b) (progn c d))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-progn")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 2"));
}

#[test]
fn cli_does_not_flag_a_single_form_inner_progn() {
    let dir = fresh_temp_dir("nested-progn-report-single");
    let file = dir.join("a.lisp");
    // (progn x) is the redundant-progn rule's job, not this one.
    fs::write(&file, "(progn (a) (progn x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-progn")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_does_not_flag_a_progn_in_a_value_slot() {
    let dir = fresh_temp_dir("nested-progn-report-value");
    let file = dir.join("a.lisp");
    // A multi-form progn as an if branch is meaningful, not nested-in-progn.
    fs::write(&file, "(if c (progn a b) d)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-progn")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_nested_progn_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("nested-progn-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(progn a (progn b c))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-progn")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "nested-progn-report policy failed",
        ));
}

#[test]
fn cli_nested_progn_expands_directory_inputs() {
    let dir = fresh_temp_dir("nested-progn-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f () (progn (log) (progn (x) (y))))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-progn")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}
