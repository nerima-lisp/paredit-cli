use super::*;

#[test]
fn cli_reports_two_identical_methods_with_the_same_specializers() {
    let dir = fresh_temp_dir("duplicate-method-report");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    fs::write(&a_file, "(defmethod area ((shape circle)) 1)\n").expect("write a.lisp");
    fs::write(&b_file, "(defmethod area ((shape circle)) 2)\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-methods")
        .arg("--output")
        .arg("json")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 1"))
        .stdout(predicate::str::contains("\"area\""));
}

#[test]
fn cli_does_not_flag_methods_with_different_specializers() {
    let dir = fresh_temp_dir("duplicate-method-report-overload");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    fs::write(&a_file, "(defmethod area ((shape circle)) 1)\n").expect("write a.lisp");
    fs::write(&b_file, "(defmethod area ((shape square)) 2)\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-methods")
        .arg("--output")
        .arg("json")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 0"));
}

#[test]
fn cli_duplicate_methods_fail_on_duplicate_trips_gate() {
    let dir = fresh_temp_dir("duplicate-method-report-gate");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    fs::write(&a_file, "(defmethod area ((shape circle)) 1)\n").expect("write a.lisp");
    fs::write(&b_file, "(defmethod area ((shape circle)) 2)\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-methods")
        .arg("--fail-on-duplicate")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "duplicate-method-report policy failed",
        ));
}

#[test]
fn cli_duplicate_methods_passes_gate_for_a_valid_overload() {
    let dir = fresh_temp_dir("duplicate-method-report-gate-clean");
    let a_file = dir.join("a.lisp");
    let b_file = dir.join("b.lisp");
    fs::write(&a_file, "(defmethod area ((shape circle)) 1)\n").expect("write a.lisp");
    fs::write(&b_file, "(defmethod area :before ((shape circle)) 2)\n").expect("write b.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-methods")
        .arg("--fail-on-duplicate")
        .arg("--output")
        .arg("json")
        .arg(&a_file)
        .arg(&b_file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 0"));
}
