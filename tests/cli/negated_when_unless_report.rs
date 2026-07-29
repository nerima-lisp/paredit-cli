use super::*;

#[test]
fn cli_flags_when_with_a_not_test() {
    let dir = fresh_temp_dir("negated-when-unless-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when (not ready) (go))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-when-unless")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"conditional_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        // The head is the `kind` and still its own field, as the old JSON had.
        .stdout(predicate::str::contains("\"kind\": \"when\""))
        .stdout(predicate::str::contains("\"head\": \"when\""))
        .stdout(predicate::str::contains("\"suggested_head\": \"unless\""));
}

#[test]
fn cli_flags_unless_with_a_null_test() {
    let dir = fresh_temp_dir("negated-when-unless-report-null");
    let file = dir.join("a.lisp");
    fs::write(&file, "(unless (null lst) (use lst))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-when-unless")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"kind\": \"unless\""))
        .stdout(predicate::str::contains("\"negator\": \"null\""))
        .stdout(predicate::str::contains("\"suggested_head\": \"when\""));
}

#[test]
fn cli_does_not_flag_a_plain_test() {
    let dir = fresh_temp_dir("negated-when-unless-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when ready (go))\n(unless done (wait))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-when-unless")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no negated test among two
        // conditionals" from "no when/unless at all".
        .stdout(predicate::str::contains("\"conditional_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("negated-when-unless-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn go [] (when (not ready) (run)))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "negated-when-unless", "--output", "json"])
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
fn cli_negated_when_unless_emits_sarif() {
    let dir = fresh_temp_dir("negated-when-unless-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when (not ready) (go))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "negated-when-unless", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        // The head is the `kind`, so `when` and `unless` are separate rules to
        // a SARIF consumer.
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/negated-when-unless/when\"",
        ))
        .stdout(predicate::str::contains(
            "when test is (not …); use unless on the un-negated test",
        ));
}

#[test]
fn cli_does_not_flag_a_malformed_negation() {
    let dir = fresh_temp_dir("negated-when-unless-report-arity");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when (not a b) c)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-when-unless")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        .stdout(predicate::str::contains("\"conditional_form_count\": 1"));
}

#[test]
fn cli_negated_when_unless_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("negated-when-unless-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when (not x) y)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-when-unless")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "negated-when-unless-report policy failed",
        ));
}

#[test]
fn cli_negated_when_unless_expands_directory_inputs() {
    let dir = fresh_temp_dir("negated-when-unless-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f () (unless (not ok) (run)))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("negated-when-unless")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
