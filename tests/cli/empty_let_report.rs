use super::*;

#[test]
fn cli_flags_empty_let() {
    let dir = fresh_temp_dir("empty-let-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let () (foo) (bar))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("empty-let")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_bindings_declare_or_let_star() {
    let dir = fresh_temp_dir("empty-let-report-clean");
    let file = dir.join("a.lisp");
    // A real binding, a leading declare (invalid in progn), and let* are all left alone.
    fs::write(
        &file,
        "(let ((x 1)) x)\n(let () (declare (ignore y)) (z))\n(let* () (w))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("empty-let")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_empty_let_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("empty-let-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let nil (run))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("empty-let")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("empty-let-report policy failed"));
}

#[test]
fn cli_lint_fix_rewrites_as_progn() {
    let dir = fresh_temp_dir("empty-let-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(let () (step) (finish))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("empty-let")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(progn (step) (finish))\n");
}
