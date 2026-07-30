use super::*;

#[test]
fn cli_reports_a_non_final_nil_in_and() {
    let dir = fresh_temp_dir("dead-boolean-operand-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(and (ready a) nil (finalize b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("dead-boolean-operand")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"boolean_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"head\": \"and\""))
        .stdout(predicate::str::contains("\"constant\": \"nil\""));
}

#[test]
fn cli_reports_a_non_final_t_in_or() {
    let dir = fresh_temp_dir("dead-boolean-operand-report-or");
    let file = dir.join("a.lisp");
    fs::write(&file, "(or (cached x) t (compute x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("dead-boolean-operand")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"head\": \"or\""))
        .stdout(predicate::str::contains("\"constant\": \"t\""));
}

#[test]
fn cli_does_not_flag_the_trailing_default_idiom() {
    let dir = fresh_temp_dir("dead-boolean-operand-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(or x y t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("dead-boolean-operand")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no dead operand in one `or` form"
        // from "no boolean form at all".
        .stdout(predicate::str::contains("\"boolean_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_dead_boolean_operand_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("dead-boolean-operand-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn check [a b] (and a nil b))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "dead-boolean-operand", "--output", "json"])
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
fn cli_dead_boolean_operand_emits_sarif() {
    let dir = fresh_temp_dir("dead-boolean-operand-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(and a nil b)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "dead-boolean-operand", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/dead-boolean-operand/dead-boolean-operand\"",
        ))
        .stdout(predicate::str::contains(
            "and short-circuits at literal nil; later operands are dead",
        ));
}

#[test]
fn cli_dead_boolean_operand_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("dead-boolean-operand-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(and a nil b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("dead-boolean-operand")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "dead-boolean-operand-report policy failed",
        ));
}

#[test]
fn cli_dead_boolean_operand_passes_gate_when_clean() {
    let dir = fresh_temp_dir("dead-boolean-operand-report-gate-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(and a b c)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("dead-boolean-operand")
        .arg("--fail-on-violation")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}
