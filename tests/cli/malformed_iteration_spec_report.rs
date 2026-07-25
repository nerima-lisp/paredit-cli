use super::*;

#[test]
fn cli_reports_a_one_element_spec() {
    let dir = fresh_temp_dir("malformed-iteration-spec-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(dolist (x) (print x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-iteration-spec")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"element_count\": 1"))
        .stdout(predicate::str::contains("\"dolist\""));
}

#[test]
fn cli_does_not_flag_valid_specs() {
    let dir = fresh_temp_dir("malformed-iteration-spec-report-clean");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(dolist (x items) (print x))\n(dotimes (i 10 done) (print i))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-iteration-spec")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_does_not_flag_a_feature_conditional_spec() {
    let dir = fresh_temp_dir("malformed-iteration-spec-report-feature");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(dotimes (i #+sbcl n1 #-sbcl n2 result) (print i))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-iteration-spec")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_flags_a_non_list_spec() {
    let dir = fresh_temp_dir("malformed-iteration-spec-report-nonlist");
    let file = dir.join("a.lisp");
    fs::write(&file, "(dolist x (print x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-iteration-spec")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_malformed_iteration_spec_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("malformed-iteration-spec-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(dotimes (i n r extra) (print i))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-iteration-spec")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "malformed-iteration-spec-report policy failed",
        ));
}

#[test]
fn cli_malformed_iteration_spec_expands_directory_inputs() {
    let dir = fresh_temp_dir("malformed-iteration-spec-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (xs) (dolist (x) (print x)))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("malformed-iteration-spec")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}
