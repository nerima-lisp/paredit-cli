use super::*;

#[test]
fn cli_reports_too_many_arguments() {
    let dir = fresh_temp_dir("if-arity-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if (plusp x) :pos :neg :zero)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"argument_count\": 4"));
}

#[test]
fn cli_does_not_flag_valid_if_forms() {
    let dir = fresh_temp_dir("if-arity-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if a b)\n(if a b c)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_does_not_flag_a_feature_conditional_else() {
    let dir = fresh_temp_dir("if-arity-report-feature");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if a b #+sbcl c #-sbcl d)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_if_arity_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("if-arity-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if a)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-arity")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("if-arity-report policy failed"));
}

#[test]
fn cli_if_arity_expands_directory_inputs() {
    let dir = fresh_temp_dir("if-arity-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (x) (if x 1 2 3))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-arity")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"argument_count\": 4"));
}
