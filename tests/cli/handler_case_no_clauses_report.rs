use super::*;

#[test]
fn cli_flags_clauseless() {
    let dir = fresh_temp_dir("handler-case-no-clauses-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(handler-case (compute))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("handler-case-no-clauses")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"handler_case_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        // The protected form the fix splices in, which the hand-written
        // renderer published and the envelope keeps.
        .stdout(predicate::str::contains("\"form_span\""));
}

#[test]
fn cli_does_not_flag_with_clauses() {
    let dir = fresh_temp_dir("handler-case-no-clauses-report-clauses");
    let file = dir.join("a.lisp");
    fs::write(&file, "(handler-case (compute) (error (e) nil))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("handler-case-no-clauses")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "the one handler-case here has
        // clauses" from "no handler-case at all".
        .stdout(predicate::str::contains("\"handler_case_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_handler_case_no_clauses_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("handler-case-no-clauses-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(handler-case x)\n").expect("write a.clj");

    paredit()
        .args(["inspect", "handler-case-no-clauses", "--output", "json"])
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
fn cli_handler_case_no_clauses_emits_sarif() {
    let dir = fresh_temp_dir("handler-case-no-clauses-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(handler-case (compute))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "handler-case-no-clauses", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/handler-case-no-clauses/handler-case-no-clauses\"",
        ))
        .stdout(predicate::str::contains(
            "a handler-case with no clauses is just its body; (handler-case x) is x",
        ));
}

#[test]
fn cli_handler_case_no_clauses_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("handler-case-no-clauses-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(handler-case x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("handler-case-no-clauses")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "handler-case-no-clauses-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_unwraps() {
    let dir = fresh_temp_dir("handler-case-no-clauses-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(handler-case (compute))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("handler-case-no-clauses")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(compute)\n");
}
