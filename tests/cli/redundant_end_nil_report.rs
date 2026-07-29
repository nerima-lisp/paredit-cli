use super::*;

#[test]
fn cli_flags_end_nil() {
    let dir = fresh_temp_dir("redundant-end-nil-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(find x seq :end nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-end-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"call_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"head\": \"find\""))
        // The fix input this report has always published; a consumer scripting
        // the deletion around the command depends on it.
        .stdout(predicate::str::contains("\"removal_span\""));
}

#[test]
fn cli_does_not_flag_non_nil() {
    let dir = fresh_temp_dir("redundant-end-nil-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(find x seq :end 5)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-end-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no redundant :end in one call"
        // from "no such call at all".
        .stdout(predicate::str::contains("\"call_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("redundant-end-nil-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(find x seq :end nil)\n").expect("write a.clj");

    paredit()
        .args(["inspect", "redundant-end-nil", "--output", "json"])
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
fn cli_redundant_end_nil_emits_sarif() {
    let dir = fresh_temp_dir("redundant-end-nil-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(find x seq :end nil)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "redundant-end-nil", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/redundant-end-nil/redundant-end-nil\"",
        ))
        .stdout(predicate::str::contains(
            "find :end defaults to nil; drop the explicit :end nil",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-end-nil-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(find x seq :end nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-end-nil")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-end-nil-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_end() {
    let dir = fresh_temp_dir("redundant-end-nil-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(remove x seq :end nil :from-end t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-end-nil")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(remove x seq :from-end t)\n");
}
