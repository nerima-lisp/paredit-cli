use super::*;

#[test]
fn cli_flags_gethash_nil_default() {
    let dir = fresh_temp_dir("gethash-default-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(gethash k table nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("gethash-default")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_non_nil_default() {
    let dir = fresh_temp_dir("gethash-default-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(gethash k table 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("gethash-default")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("gethash-default-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(gethash k h nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("gethash-default")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "gethash-default-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_default() {
    let dir = fresh_temp_dir("gethash-default-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(gethash k table nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("gethash-default")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(gethash k table)\n");
}
