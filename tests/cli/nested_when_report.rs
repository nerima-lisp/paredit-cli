use super::*;

#[test]
fn cli_flags_when_in_when() {
    let dir = fresh_temp_dir("nested-when-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when a (when b (do-it)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-when")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_extra_body_or_non_when() {
    let dir = fresh_temp_dir("nested-when-report-clean");
    let file = dir.join("a.lisp");
    // Extra outer body form (d not guarded by b); non-when inner body.
    fs::write(&file, "(when a (when b c) d)\n(when a (unless b c))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-when")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_nested_when_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("nested-when-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when ready (when armed (fire)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-when")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("nested-when-report policy failed"));
}

#[test]
fn cli_lint_fix_merges_tests_with_and() {
    let dir = fresh_temp_dir("nested-when-report-fix");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(when (ready-p x) (when (> n 0) (step n) (log n)))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("nested-when")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(when (and (ready-p x) (> n 0)) (step n) (log n))\n");
}
