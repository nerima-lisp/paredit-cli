use super::*;

#[test]
fn cli_reports_eq_of_a_variable_with_itself() {
    let dir = fresh_temp_dir("self-comparison-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when (eq status status) 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("self-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"eq\""));
}

#[test]
fn cli_reports_an_ordering_predicate_of_identical_operands() {
    let dir = fresh_temp_dir("self-comparison-report-order");
    let file = dir.join("a.lisp");
    fs::write(&file, "(< (rank a) (rank a))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("self-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("(rank a)"));
}

#[test]
fn cli_does_not_flag_distinct_operands() {
    let dir = fresh_temp_dir("self-comparison-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eq x y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("self-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_does_not_flag_the_nan_check_idiom() {
    let dir = fresh_temp_dir("self-comparison-report-nan");
    let file = dir.join("a.lisp");
    fs::write(&file, "(/= x x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("self-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_self_comparison_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("self-comparison-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eql x x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("self-comparison")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "self-comparison-report policy failed",
        ));
}
