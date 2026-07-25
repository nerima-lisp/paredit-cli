use super::*;

#[test]
fn cli_reports_too_few_arguments() {
    let dir = fresh_temp_dir("equality-arity-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("equality-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"eq\""))
        .stdout(predicate::str::contains("\"argument_count\": 1"));
}

#[test]
fn cli_reports_too_many_arguments() {
    let dir = fresh_temp_dir("equality-arity-report-many");
    let file = dir.join("a.lisp");
    fs::write(&file, "(equal a b c)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("equality-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"argument_count\": 3"));
}

#[test]
fn cli_does_not_flag_binary_or_variadic_comparisons() {
    let dir = fresh_temp_dir("equality-arity-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq a b)\n(= x y z)\n(< n)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("equality-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_does_not_flag_a_reader_conditional_argument() {
    let dir = fresh_temp_dir("equality-arity-report-feature");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eql p #+sbcl q #-sbcl r)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("equality-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_equality_arity_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("equality-arity-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("equality-arity")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "equality-arity-report policy failed",
        ));
}

#[test]
fn cli_equality_arity_expands_directory_inputs() {
    let dir = fresh_temp_dir("equality-arity-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (x) (when (eq x) 1))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("equality-arity")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}
