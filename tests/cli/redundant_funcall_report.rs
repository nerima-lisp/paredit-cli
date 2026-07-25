use super::*;

#[test]
fn cli_flags_sharp_quoted_funcall() {
    let dir = fresh_temp_dir("redundant-funcall-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun run (x) (funcall #'process x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-funcall")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"callee\": \"process\""));
}

#[test]
fn cli_does_not_flag_variable_lambda_or_quote() {
    let dir = fresh_temp_dir("redundant-funcall-report-clean");
    let file = dir.join("a.lisp");
    // Variable fn, sharp-quoted lambda, ordinary-quoted symbol, and apply.
    fs::write(
        &file,
        "(funcall fn a)\n(funcall #'(lambda (x) x) y)\n(funcall 'foo b)\n(apply #'g args)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-funcall")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_redundant_funcall_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-funcall-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(funcall #'handler event)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-funcall")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-funcall-report policy failed",
        ));
}

#[test]
fn cli_redundant_funcall_expands_directory_inputs() {
    let dir = fresh_temp_dir("redundant-funcall-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(funcall #'foo)\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-funcall")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_lint_fix_rewrites_funcall_as_direct_call() {
    let dir = fresh_temp_dir("redundant-funcall-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun run (x) (funcall #'process x 42))\n").expect("write a.lisp");

    // The aggregator's --fix engine should rewrite the funcall as a direct call.
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-funcall")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(defun run (x) (process x 42))\n");
}
