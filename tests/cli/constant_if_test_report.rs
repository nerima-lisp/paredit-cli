use super::*;

#[test]
fn cli_flags_constant_true_test() {
    let dir = fresh_temp_dir("constant-if-test-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun pick () (if t 1 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("constant-if-test")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"test\": \"t\""));
}

#[test]
fn cli_does_not_flag_variable_or_truthy_literal_test() {
    let dir = fresh_temp_dir("constant-if-test-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if ready a b)\n(if 5 a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("constant-if-test")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_constant_if_test_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("constant-if-test-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if nil dead alive)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("constant-if-test")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "constant-if-test-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_the_dead_branch() {
    let dir = fresh_temp_dir("constant-if-test-report-fix");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(if t (do-a x) (do-b y))\n(if nil (do-a x) (do-b y))\n(if nil (side-effect))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("constant-if-test")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(do-a x)\n(do-b y)\nnil\n");
}

#[test]
fn cli_lint_marks_constant_if_test_as_dead_code() {
    let dir = fresh_temp_dir("constant-if-test-report-lint");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if t a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("constant-if-test")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"category\": \"dead-code\""))
        .stdout(predicate::str::contains("\"fixable\": true"));
}
