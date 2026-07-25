use super::*;

#[test]
fn cli_flags_unless_in_unless() {
    let dir = fresh_temp_dir("nested-unless-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(unless a (unless b (do-it)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-unless")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_extra_body_or_non_unless() {
    let dir = fresh_temp_dir("nested-unless-report-clean");
    let file = dir.join("a.lisp");
    // Extra outer body form (d not guarded by b); non-unless inner body.
    fs::write(&file, "(unless a (unless b c) d)\n(unless a (when b c))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-unless")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_nested_unless_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("nested-unless-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(unless done (unless paused (tick)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-unless")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "nested-unless-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_merges_tests_with_or() {
    let dir = fresh_temp_dir("nested-unless-report-fix");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(unless (done-p x) (unless (< n 0) (step n) (log n)))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("nested-unless")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(unless (or (done-p x) (< n 0)) (step n) (log n))\n");
}
