use super::*;

#[test]
fn cli_flags_bare_nil_key() {
    let dir = fresh_temp_dir("case-nil-key-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x (nil 1) (t 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("case-nil-key")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"case_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"head\": \"case\""));
}

#[test]
fn cli_does_not_flag_nil_key_list_or_quoted_nil() {
    let dir = fresh_temp_dir("case-nil-key-report-clean");
    let file = dir.join("a.lisp");
    // ((nil) …) is the correct spelling; 'nil is quoted-case-key's concern;
    // ordinary keys are fine.
    fs::write(
        &file,
        "(case x ((nil) 1))\n(case y ('nil 1))\n(case z (a 1) (t 2))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("case-nil-key")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no bare nil key in three `case`
        // forms" from "no `case` form at all".
        .stdout(predicate::str::contains("\"case_form_count\": 3"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_case_nil_key_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("case-nil-key-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn pick [] (case x (nil 1)))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "case-nil-key", "--output", "json"])
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
fn cli_case_nil_key_emits_sarif() {
    let dir = fresh_temp_dir("case-nil-key-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x (nil 1) (t 2))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "case-nil-key", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/case-nil-key/case-nil-key\"",
        ))
        .stdout(predicate::str::contains(
            "case clause key nil is the empty key list and never matches",
        ));
}

#[test]
fn cli_case_nil_key_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("case-nil-key-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(ecase state (nil (idle)) (running (tick)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("case-nil-key")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "case-nil-key-report policy failed",
        ));
}

#[test]
fn cli_lint_reports_case_nil_key_as_an_error() {
    // The aggregate lint reports it and, being error-severity, --fail-on error trips.
    let dir = fresh_temp_dir("case-nil-key-report-lint");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x (nil 1) (t 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("case-nil-key")
        .arg("--fail-on")
        .arg("error")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("lint-report policy failed"));
}
