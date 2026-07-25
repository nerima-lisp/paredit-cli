use super::*;

#[test]
fn cli_flags_cleanupless() {
    let dir = fresh_temp_dir("unwind-protect-no-cleanup-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(unwind-protect (compute))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unwind-protect-no-cleanup")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_with_cleanup() {
    let dir = fresh_temp_dir("unwind-protect-no-cleanup-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(unwind-protect (compute) (cleanup))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unwind-protect-no-cleanup")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_unwind_protect_no_cleanup_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("unwind-protect-no-cleanup-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(unwind-protect x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("unwind-protect-no-cleanup")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "unwind-protect-no-cleanup-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_unwraps() {
    let dir = fresh_temp_dir("unwind-protect-no-cleanup-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(unwind-protect (compute))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("unwind-protect-no-cleanup")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(compute)\n");
}
