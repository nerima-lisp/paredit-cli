use super::*;

#[test]
fn cli_flags_start_zero() {
    let dir = fresh_temp_dir("redundant-start-zero-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(find x seq :start 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-start-zero")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_nonzero() {
    let dir = fresh_temp_dir("redundant-start-zero-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(find x seq :start 2)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-start-zero")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-start-zero-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(find x seq :start 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-start-zero")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-start-zero-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_start() {
    let dir = fresh_temp_dir("redundant-start-zero-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(remove x seq :start 0 :from-end t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-start-zero")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(remove x seq :from-end t)\n");
}
