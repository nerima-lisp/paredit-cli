use super::*;

#[test]
fn cli_flags_single_arg_append() {
    let dir = fresh_temp_dir("single-operand-list-op-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(append xs)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-operand-list-op")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"list_op_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"head\": \"append\""));
}

#[test]
fn cli_does_not_flag_multi_arg_or_numeric_op() {
    let dir = fresh_temp_dir("single-operand-list-op-report-clean");
    let file = dir.join("a.lisp");
    // Two args, zero args, and a numeric op (type-checking, not covered) are left alone.
    fs::write(&file, "(append a b)\n(append)\n(max x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-operand-list-op")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator counts the two `append`s and not `(max x)`, which
        // this rule deliberately does not cover.
        .stdout(predicate::str::contains("\"list_op_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_single_operand_list_op_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("single-operand-list-op-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [xs] (append xs))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "single-operand-list-op", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

/// The envelope's interchange formats, which this report reached by moving onto
/// it. Asserted here only far enough to prove the command accepts them; their
/// content is covered once in `report_interop`.
#[test]
fn cli_single_operand_list_op_emits_sarif() {
    let dir = fresh_temp_dir("single-operand-list-op-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nconc items)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "single-operand-list-op", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/single-operand-list-op/single-operand-list-op\"",
        ))
        .stdout(predicate::str::contains(
            "nconc of one argument returns it unchanged; (nconc x) is x",
        ));
}

#[test]
fn cli_single_operand_list_op_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("single-operand-list-op-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nconc items)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-operand-list-op")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "single-operand-list-op-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_unwraps_to_the_argument() {
    let dir = fresh_temp_dir("single-operand-list-op-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(append (mapcar #'f xs))\n(list* tail)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("single-operand-list-op")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(mapcar #'f xs)\ntail\n");
}
