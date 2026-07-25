use super::*;

#[test]
fn cli_reports_a_repeated_test_expression() {
    let dir = fresh_temp_dir("duplicate-cond-test-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((foo) 1) ((bar) 2) ((foo) 3))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-cond-tests")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 1"))
        .stdout(predicate::str::contains("(foo)"));
}

#[test]
fn cli_finds_a_cond_nested_in_a_function_body() {
    let dir = fresh_temp_dir("duplicate-cond-test-report-nested");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (cond ((p x) 1) ((p x) 2)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-cond-tests")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 1"));
}

#[test]
fn cli_does_not_flag_distinct_tests() {
    let dir = fresh_temp_dir("duplicate-cond-test-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((foo) 1) ((bar) 2) (t 3))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-cond-tests")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 0"));
}

#[test]
fn cli_duplicate_cond_tests_fail_on_duplicate_trips_gate() {
    let dir = fresh_temp_dir("duplicate-cond-test-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((foo) 1) ((foo) 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-cond-tests")
        .arg("--fail-on-duplicate")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "duplicate-cond-test-report policy failed",
        ));
}

#[test]
fn cli_duplicate_cond_tests_passes_gate_when_all_tests_are_distinct() {
    let dir = fresh_temp_dir("duplicate-cond-test-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond ((foo) 1) ((bar) 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-cond-tests")
        .arg("--fail-on-duplicate")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"duplicate_count\": 0"));
}
