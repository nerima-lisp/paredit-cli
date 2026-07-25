use super::*;

#[test]
fn cli_flags_single_arg_less_than() {
    let dir = fresh_temp_dir("single-arg-comparison-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (when (< x) (go)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-arg-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"<\""));
}

#[test]
fn cli_flags_single_arg_equal() {
    let dir = fresh_temp_dir("single-arg-comparison-report-eq");
    let file = dir.join("a.lisp");
    fs::write(&file, "(= (length xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-arg-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"=\""));
}

#[test]
fn cli_does_not_flag_two_argument_comparison() {
    let dir = fresh_temp_dir("single-arg-comparison-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(< x y)\n(<= a b c)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-arg-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_does_not_flag_equality_predicates() {
    let dir = fresh_temp_dir("single-arg-comparison-report-eql");
    let file = dir.join("a.lisp");
    // (eql x) is the equality-arity rule's territory, not this one.
    fs::write(&file, "(eql x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-arg-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_single_arg_comparison_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("single-arg-comparison-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(> n)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-arg-comparison")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "single-arg-comparison-report policy failed",
        ));
}

#[test]
fn cli_single_arg_comparison_expands_directory_inputs() {
    let dir = fresh_temp_dir("single-arg-comparison-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (n) (/= n))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-arg-comparison")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"));
}
