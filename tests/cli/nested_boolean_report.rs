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
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        // Two `or` forms scanned: the outer and the nested one.
        .stdout(predicate::str::contains("\"boolean_form_count\": 2"))
        .stdout(predicate::str::contains("\"line\": 1"))
        // The operator is the `kind` and still its own field, as the old JSON
        // had.
        .stdout(predicate::str::contains("\"kind\": \"or\""))
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
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no redundant nesting among four
        // boolean forms" from "no boolean form at all".
        .stdout(predicate::str::contains("\"boolean_form_count\": 4"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("nested-boolean-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn any [] (or a (or b c) d))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "nested-boolean", "--output", "json"])
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
fn cli_nested_boolean_emits_sarif() {
    let dir = fresh_temp_dir("nested-boolean-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(and p (and q r))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "nested-boolean", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        // The operator is the `kind`, so `and` and `or` are separate rules to a
        // SARIF consumer.
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/nested-boolean/and\"",
        ))
        .stdout(predicate::str::contains(
            "and nested in a and flattens; its operands splice in",
        ));
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
