use super::*;

#[test]
fn cli_reports_incf_with_too_many_arguments() {
    let dir = fresh_temp_dir("modify-macro-arity-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(incf counter 1 2)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("modify-macro-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"incf\""))
        .stdout(predicate::str::contains("\"1 or 2\""));
}

#[test]
fn cli_does_not_flag_valid_calls() {
    let dir = fresh_temp_dir("modify-macro-arity-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(incf x)\n(incf y 2)\n(push a stack)\n(pop stack)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("modify-macro-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_does_not_flag_a_reader_conditional_argument() {
    let dir = fresh_temp_dir("modify-macro-arity-report-feature");
    let file = dir.join("a.lisp");
    fs::write(&file, "(decf z #+sbcl 1 #-sbcl 2)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("modify-macro-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_flags_push_with_too_few_arguments() {
    let dir = fresh_temp_dir("modify-macro-arity-report-push");
    let file = dir.join("a.lisp");
    fs::write(&file, "(push item)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("modify-macro-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"exactly 2\""));
}

#[test]
fn cli_modify_macro_arity_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("modify-macro-arity-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(pop)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("modify-macro-arity")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "modify-macro-arity-report policy failed",
        ));
}

#[test]
fn cli_modify_macro_arity_expands_directory_inputs() {
    let dir = fresh_temp_dir("modify-macro-arity-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (x) (incf x 1 2))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("modify-macro-arity")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}
