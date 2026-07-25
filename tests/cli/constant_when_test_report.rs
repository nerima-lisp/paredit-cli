use super::*;

#[test]
fn cli_flags_when_t() {
    let dir = fresh_temp_dir("constant-when-test-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when t (do-it))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("constant-when-test")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"head\": \"when\""))
        .stdout(predicate::str::contains("\"always_runs\": true"));
}

#[test]
fn cli_flags_unless_t_as_dead() {
    let dir = fresh_temp_dir("constant-when-test-report-dead");
    let file = dir.join("a.lisp");
    fs::write(&file, "(unless t (unreachable))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("constant-when-test")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"always_runs\": false"));
}

#[test]
fn cli_does_not_flag_a_variable_test() {
    let dir = fresh_temp_dir("constant-when-test-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when ready a)\n(when 5 b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("constant-when-test")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_constant_when_test_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("constant-when-test-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(unless nil x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("constant-when-test")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "constant-when-test-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_always_and_dead_cases() {
    let dir = fresh_temp_dir("constant-when-test-report-fix");
    let file = dir.join("a.lisp");
    // `(when t …)` splices to progn; `(when nil …)` collapses to nil.
    fs::write(&file, "(unless nil (a) (b))\n(when nil (dead))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("constant-when-test")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(progn (a) (b))\nnil\n");
}
