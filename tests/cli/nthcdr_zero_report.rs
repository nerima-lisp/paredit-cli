use super::*;

#[test]
fn cli_flags_nthcdr_zero() {
    let dir = fresh_temp_dir("nthcdr-zero-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nthcdr 0 items)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nthcdr-zero")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_nonzero_or_float() {
    let dir = fresh_temp_dir("nthcdr-zero-report-clean");
    let file = dir.join("a.lisp");
    // A non-0 index, a float 0.0, and a variable index are all left alone.
    fs::write(&file, "(nthcdr 1 x)\n(nthcdr 0.0 x)\n(nthcdr n x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nthcdr-zero")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_nthcdr_zero_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("nthcdr-zero-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nthcdr 0 lst)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nthcdr-zero")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("nthcdr-zero-report policy failed"));
}

#[test]
fn cli_lint_fix_unwraps_to_the_list() {
    let dir = fresh_temp_dir("nthcdr-zero-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nthcdr 0 (compute))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("nthcdr-zero")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(compute)\n");
}
