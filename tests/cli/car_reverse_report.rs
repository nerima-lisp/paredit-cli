use super::*;

#[test]
fn cli_flags_car_reverse() {
    let dir = fresh_temp_dir("car-reverse-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (reverse items))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("car-reverse")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_nreverse() {
    let dir = fresh_temp_dir("car-reverse-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (nreverse xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("car-reverse")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("car-reverse-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (reverse xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("car-reverse")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("car-reverse-report policy failed"));
}

#[test]
fn cli_lint_fix_rewrites_to_last() {
    let dir = fresh_temp_dir("car-reverse-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (reverse xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("car-reverse")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(car (last xs))\n");
}
