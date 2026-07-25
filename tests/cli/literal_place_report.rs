use super::*;

#[test]
fn cli_flags_incf_of_a_literal() {
    let dir = fresh_temp_dir("literal-place-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f () (incf 5))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("literal-place")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"incf\""))
        .stdout(predicate::str::contains("\"place\": \"5\""));
}

#[test]
fn cli_flags_setf_and_psetf_literal_places() {
    let dir = fresh_temp_dir("literal-place-report-setf");
    let file = dir.join("a.lisp");
    // setf place at an odd index; psetf place in the second pair.
    fs::write(
        &file,
        "(setf 5 x)\n(psetf a 1 :k 2)\n(setf ok 1 (car y) 2)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("literal-place")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        // (setf 5 x) and (psetf ... :k ...) flag; the valid setf does not.
        .stdout(predicate::str::contains("\"violation_count\": 2"))
        .stdout(predicate::str::contains("\"operator\": \"setf\""))
        .stdout(predicate::str::contains("\"operator\": \"psetf\""));
}

#[test]
fn cli_flags_push_into_a_literal_place() {
    let dir = fresh_temp_dir("literal-place-report-push");
    let file = dir.join("a.lisp");
    fs::write(&file, "(push item 3)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("literal-place")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"push\""));
}

#[test]
fn cli_does_not_flag_variable_or_accessor_places() {
    let dir = fresh_temp_dir("literal-place-report-clean");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(incf n)\n(push x (gethash k table))\n(push 3 stack)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("literal-place")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_literal_place_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("literal-place-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(decf :k)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("literal-place")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "literal-place-report policy failed",
        ));
}

#[test]
fn cli_literal_place_expands_directory_inputs() {
    let dir = fresh_temp_dir("literal-place-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f () (pushnew x 42))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("literal-place")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}
