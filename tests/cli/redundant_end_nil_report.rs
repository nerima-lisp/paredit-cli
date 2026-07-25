use super::*;

#[test]
fn cli_flags_end_nil() {
    let dir = fresh_temp_dir("redundant-end-nil-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(find x seq :end nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-end-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"head\": \"find\""));
}

#[test]
fn cli_does_not_flag_non_nil() {
    let dir = fresh_temp_dir("redundant-end-nil-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(find x seq :end 5)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-end-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-end-nil-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(find x seq :end nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-end-nil")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-end-nil-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_end() {
    let dir = fresh_temp_dir("redundant-end-nil-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(remove x seq :end nil :from-end t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-end-nil")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(remove x seq :from-end t)\n");
}
