use super::*;

#[test]
fn cli_flags_floor_by_one() {
    let dir = fresh_temp_dir("redundant-divisor-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(floor total 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-divisor")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"floor\""));
}

#[test]
fn cli_does_not_flag_non_one_divisor() {
    let dir = fresh_temp_dir("redundant-divisor-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(floor x 2)\n(mod x 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-divisor")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_redundant_divisor_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-divisor-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(round x 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-divisor")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-divisor-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_divisor() {
    let dir = fresh_temp_dir("redundant-divisor-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(truncate (+ a b) 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-divisor")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(truncate (+ a b))\n");
}
