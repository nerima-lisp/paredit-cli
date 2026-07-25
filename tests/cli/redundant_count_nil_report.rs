use super::*;

#[test]
fn cli_flags_count_nil() {
    let dir = fresh_temp_dir("redundant-count-nil-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(remove x seq :count nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-count-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_non_nil() {
    let dir = fresh_temp_dir("redundant-count-nil-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(remove x seq :count 3)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-count-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-count-nil-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(remove x seq :count nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-count-nil")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-count-nil-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_count() {
    let dir = fresh_temp_dir("redundant-count-nil-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(delete x seq :count nil :from-end t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-count-nil")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(delete x seq :from-end t)\n");
}
