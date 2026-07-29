use super::*;

#[test]
fn cli_flags_and_of_nots() {
    let dir = fresh_temp_dir("de-morgan-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun neither? (a b) (and (not a) (not b)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("de-morgan")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"boolean_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"and\""));
}

#[test]
fn cli_does_not_flag_mixed_operands() {
    let dir = fresh_temp_dir("de-morgan-report-clean");
    let file = dir.join("a.lisp");
    // Mixed negated/plain, single negation, and a multi-arg not.
    fs::write(
        &file,
        "(and (not a) b)\n(and (not a))\n(or (not a b) (not c))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("de-morgan")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no collapsible boolean in three
        // `and`/`or` forms" from "no boolean form at all".
        .stdout(predicate::str::contains("\"boolean_form_count\": 3"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_de_morgan_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("de-morgan-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn neither [a b] (and (not a) (not b)))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "de-morgan", "--output", "json"])
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
fn cli_de_morgan_emits_sarif() {
    let dir = fresh_temp_dir("de-morgan-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(or (not a) (not b))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "de-morgan", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/de-morgan/or\"",
        ))
        .stdout(predicate::str::contains(
            "or of negations collapses by De Morgan to (not (and …))",
        ));
}

#[test]
fn cli_de_morgan_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("de-morgan-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(or (not ready) (not able))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("de-morgan")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("de-morgan-report policy failed"));
}

#[test]
fn cli_lint_fix_collapses_both_directions() {
    let dir = fresh_temp_dir("de-morgan-report-fix");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(and (not a) (not b))\n(or (not (foo x)) (not c))\n(and (not a) (not b) (not c))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("de-morgan")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(
        fixed,
        "(not (or a b))\n(not (and (foo x) c))\n(not (or a b c))\n"
    );
}
