use super::*;

#[test]
fn cli_flags_return_nil() {
    let dir = fresh_temp_dir("explicit-nil-return-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(loop (when done (return nil)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("explicit-nil-return")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"return\""));
}

#[test]
fn cli_does_not_flag_non_nil_result_or_block_only() {
    let dir = fresh_temp_dir("explicit-nil-return-report-clean");
    let file = dir.join("a.lisp");
    // A non-nil result, a bare return-from, and (return-from nil) (nil is the
    // block name) are all left alone.
    fs::write(&file, "(return 5)\n(return-from foo)\n(return-from nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("explicit-nil-return")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_explicit_nil_return_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("explicit-nil-return-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(return-from search nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("explicit-nil-return")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "explicit-nil-return-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_the_nil_result() {
    let dir = fresh_temp_dir("explicit-nil-return-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(return nil)\n(return-from outer nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("explicit-nil-return")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(return)\n(return-from outer)\n");
}
