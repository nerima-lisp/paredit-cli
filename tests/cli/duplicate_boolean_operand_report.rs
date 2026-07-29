use super::*;

#[test]
fn cli_reports_a_repeated_operand_in_an_or() {
    let dir = fresh_temp_dir("duplicate-boolean-operand-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(or x y x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-boolean-operands")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"boolean_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"head\": \"or\""))
        .stdout(predicate::str::contains("\"operand\": \"x\""))
        .stdout(predicate::str::contains("\"occurrence_count\": 2"));
}

#[test]
fn cli_finds_a_boolean_nested_in_a_function_body() {
    let dir = fresh_temp_dir("duplicate-boolean-operand-report-nested");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (x) (when (or (p x) (p x)) 1))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-boolean-operands")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}

#[test]
fn cli_does_not_flag_distinct_operands() {
    let dir = fresh_temp_dir("duplicate-boolean-operand-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(and a b c)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-boolean-operands")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no repeat in one `and` form" from
        // "no boolean form at all".
        .stdout(predicate::str::contains("\"boolean_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_duplicate_boolean_operands_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("duplicate-boolean-operand-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn check [x] (or x x))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "duplicate-boolean-operands", "--output", "json"])
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
fn cli_duplicate_boolean_operands_emits_sarif() {
    let dir = fresh_temp_dir("duplicate-boolean-operand-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(or x y x)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "duplicate-boolean-operands", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/duplicate-boolean-operands/duplicate-boolean-operands\"",
        ))
        .stdout(predicate::str::contains("or repeats operand x (2×)"));
}

#[test]
fn cli_duplicate_boolean_operands_fail_on_duplicate_trips_gate() {
    let dir = fresh_temp_dir("duplicate-boolean-operand-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(or x x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-boolean-operands")
        .arg("--fail-on-duplicate")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "duplicate-boolean-operand-report policy failed",
        ));
}

#[test]
fn cli_duplicate_boolean_operands_passes_gate_when_all_operands_are_distinct() {
    let dir = fresh_temp_dir("duplicate-boolean-operand-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(or a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("duplicate-boolean-operands")
        .arg("--fail-on-duplicate")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}
