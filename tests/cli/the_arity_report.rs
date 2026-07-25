use super::*;

#[test]
fn cli_reports_too_few_arguments() {
    let dir = fresh_temp_dir("the-arity-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(the fixnum)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("the-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"argument_count\": 1"));
}

#[test]
fn cli_reports_too_many_arguments() {
    let dir = fresh_temp_dir("the-arity-report-many");
    let file = dir.join("a.lisp");
    fs::write(&file, "(the fixnum x y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("the-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"argument_count\": 3"));
}

#[test]
fn cli_does_not_flag_a_valid_the() {
    let dir = fresh_temp_dir("the-arity-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(the fixnum (+ a b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("the-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_does_not_flag_a_reader_conditional_type() {
    let dir = fresh_temp_dir("the-arity-report-feature");
    let file = dir.join("a.lisp");
    fs::write(&file, "(the #+sbcl fixnum #-sbcl integer x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("the-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_the_arity_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("the-arity-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(the fixnum)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("the-arity")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("the-arity-report policy failed"));
}

#[test]
fn cli_the_arity_expands_directory_inputs() {
    let dir = fresh_temp_dir("the-arity-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (x) (the fixnum x x))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("the-arity")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}
