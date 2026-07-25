use super::*;

#[test]
fn cli_flags_single_form_prog1() {
    let dir = fresh_temp_dir("redundant-prog1-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(prog1 (compute))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-prog1")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_multi_form() {
    let dir = fresh_temp_dir("redundant-prog1-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(prog1 a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-prog1")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-prog1-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(prog1 x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-prog1")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-prog1-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_unwraps() {
    let dir = fresh_temp_dir("redundant-prog1-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(prog1 (only x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-prog1")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(only x)\n");
}
