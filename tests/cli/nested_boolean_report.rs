use super::*;

#[test]
fn cli_flags_or_nested_in_or() {
    let dir = fresh_temp_dir("nested-boolean-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(or a (or b c) d)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-boolean")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"or\""));
}

#[test]
fn cli_does_not_flag_different_operator_or_single_operand() {
    let dir = fresh_temp_dir("nested-boolean-report-clean");
    let file = dir.join("a.lisp");
    // Mixed operators do not flatten; a single-operand inner belongs to
    // single-operand-boolean.
    fs::write(&file, "(or a (and b c))\n(or a (or x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-boolean")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"violation_count\": 0"));
}

#[test]
fn cli_nested_boolean_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("nested-boolean-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(and p (and q r))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-boolean")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "nested-boolean-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_flattens_nested_boolean() {
    let dir = fresh_temp_dir("nested-boolean-report-fix");
    let file = dir.join("a.lisp");
    // The fixpoint loop collapses both levels of nesting.
    fs::write(&file, "(or a (or b (or c d)) e)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("nested-boolean")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(or a b c d e)\n");
}
