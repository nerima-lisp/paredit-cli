use super::*;

#[test]
fn cli_flags_explicit_nil() {
    let dir = fresh_temp_dir("getf-default-nil-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(getf plist :key nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("getf-default-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_non_nil() {
    let dir = fresh_temp_dir("getf-default-nil-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(getf plist :key 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("getf-default-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("getf-default-nil-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(getf plist :key nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("getf-default-nil")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "getf-default-nil-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_default_nil() {
    let dir = fresh_temp_dir("getf-default-nil-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(getf plist :key nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("getf-default-nil")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(getf plist :key)\n");
}
