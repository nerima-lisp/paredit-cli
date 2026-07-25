use super::*;

#[test]
fn cli_flags_car_nthcdr() {
    let dir = fresh_temp_dir("car-nthcdr-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (nthcdr n items))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("car-nthcdr")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_cdr_outer() {
    let dir = fresh_temp_dir("car-nthcdr-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cdr (nthcdr n x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("car-nthcdr")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("car-nthcdr-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (nthcdr n x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("car-nthcdr")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("car-nthcdr-report policy failed"));
}

#[test]
fn cli_lint_fix_rewrites_to_nth() {
    let dir = fresh_temp_dir("car-nthcdr-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(car (nthcdr (+ i 1) xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("car-nthcdr")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(nth (+ i 1) xs)\n");
}
