use super::*;

#[test]
fn cli_reports_gethash_missing_the_table() {
    let dir = fresh_temp_dir("accessor-arity-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(gethash key)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("accessor-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"gethash\""))
        .stdout(predicate::str::contains("\"2 or 3\""));
}

#[test]
fn cli_reports_nth_missing_the_list() {
    let dir = fresh_temp_dir("accessor-arity-report-nth");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nth n)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("accessor-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"exactly 2\""));
}

#[test]
fn cli_does_not_flag_valid_accessors() {
    let dir = fresh_temp_dir("accessor-arity-report-clean");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(nth i items)\n(elt seq 0)\n(gethash k tbl)\n(gethash k tbl d)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("accessor-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_does_not_flag_a_reader_conditional_argument() {
    let dir = fresh_temp_dir("accessor-arity-report-feature");
    let file = dir.join("a.lisp");
    fs::write(&file, "(gethash k #+sbcl a #-sbcl b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("accessor-arity")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_accessor_arity_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("accessor-arity-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(gethash key)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("accessor-arity")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "accessor-arity-report policy failed",
        ));
}

#[test]
fn cli_accessor_arity_expands_directory_inputs() {
    let dir = fresh_temp_dir("accessor-arity-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (k) (when (gethash k) 1))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("accessor-arity")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}
