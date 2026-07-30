use super::*;

#[test]
fn cli_flags_single_form_prog1() {
    let dir = fresh_temp_dir("redundant-prog1-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(prog1 (compute))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-prog1")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"prog1_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        // The inner form's span survived the move onto the envelope.
        .stdout(predicate::str::contains("\"form_span\""));
}

#[test]
fn cli_does_not_flag_multi_form() {
    let dir = fresh_temp_dir("redundant-prog1-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(prog1 a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-prog1")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "this prog1 sequences something"
        // from "no prog1 form at all".
        .stdout(predicate::str::contains("\"prog1_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("redundant-prog1-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(prog1 x)\n").expect("write a.clj");

    paredit()
        .args(["inspect", "redundant-prog1", "--output", "json"])
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
fn cli_redundant_prog1_emits_sarif() {
    let dir = fresh_temp_dir("redundant-prog1-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(prog1 x)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "redundant-prog1", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/redundant-prog1/redundant-prog1\"",
        ))
        .stdout(predicate::str::contains(
            "a prog1 wrapping a single form is just that form; (prog1 x) is x",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-prog1-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(prog1 x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-prog1")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-prog1-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_unwraps() {
    let dir = fresh_temp_dir("redundant-prog1-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(prog1 (only x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-prog1")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(only x)\n");
}
