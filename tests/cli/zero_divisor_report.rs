use super::*;

#[test]
fn cli_flags_divide_by_zero() {
    let dir = fresh_temp_dir("zero-divisor-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(/ x 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("zero-divisor")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_flags_mod_zero() {
    let dir = fresh_temp_dir("zero-divisor-report-mod");
    let file = dir.join("a.lisp");
    fs::write(&file, "(mod x 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("zero-divisor")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}

#[test]
fn cli_does_not_flag_zero_numerator() {
    let dir = fresh_temp_dir("zero-divisor-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(/ 0 x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("zero-divisor")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_zero_divisor_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("zero-divisor-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(/ x 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("zero-divisor")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "zero-divisor-report policy failed",
        ));
}
