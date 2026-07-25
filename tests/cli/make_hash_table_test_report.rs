use super::*;

#[test]
fn cli_flags_eql_test() {
    let dir = fresh_temp_dir("make-hash-table-test-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-hash-table :test 'eql)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("make-hash-table-test")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_custom_test() {
    let dir = fresh_temp_dir("make-hash-table-test-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-hash-table :test 'equal)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("make-hash-table-test")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("make-hash-table-test-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-hash-table :test 'eql)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("make-hash-table-test")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "make-hash-table-test-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_test() {
    let dir = fresh_temp_dir("make-hash-table-test-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(make-hash-table :size 16 :test 'eql)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("make-hash-table-test")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(make-hash-table :size 16)\n");
}
