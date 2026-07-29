use super::*;

#[test]
fn cli_flags_the_t() {
    let dir = fresh_temp_dir("redundant-the-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(the t (compute))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-the")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"the_form_count\": 1"))
        .stdout(predicate::str::contains("\"kind\": \"vacuous\""))
        .stdout(predicate::str::contains("\"line\": 1"))
        // The inner form's extent is the one extra field the old report
        // published, and it still crosses.
        .stdout(predicate::str::contains("\"form_span\""));
}

#[test]
fn cli_does_not_flag_a_specific_type() {
    let dir = fresh_temp_dir("redundant-the-report-clean");
    let file = dir.join("a.lisp");
    // A meaningful type assertion and a wrong-arity form are left alone.
    fs::write(&file, "(the fixnum x)\n(the t a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-the")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no vacuous `the` in two `the`
        // forms" from "no `the` form at all".
        .stdout(predicate::str::contains("\"the_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("redundant-the-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(the t x)\n").expect("write a.clj");

    paredit()
        .args(["inspect", "redundant-the", "--output", "json"])
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
fn cli_redundant_the_emits_sarif() {
    let dir = fresh_temp_dir("redundant-the-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(the t (compute))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "redundant-the", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/redundant-the/vacuous\"",
        ))
        .stdout(predicate::str::contains(
            "(the t form) is a vacuous type declaration; it is just form",
        ));
}

#[test]
fn cli_redundant_the_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-the-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(the t x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-the")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-the-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_unwraps_to_the_form() {
    let dir = fresh_temp_dir("redundant-the-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(the t (mapcar #'f xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-the")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(mapcar #'f xs)\n");
}
