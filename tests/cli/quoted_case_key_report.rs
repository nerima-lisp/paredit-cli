use super::*;

#[test]
fn cli_reports_quoted_keys() {
    let dir = fresh_temp_dir("quoted-case-key-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case sym ('apple :fruit) ('carrot :veg) (t :x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("quoted-case-key")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 2"))
        .stdout(predicate::str::contains("\"case_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("'apple"));
}

#[test]
fn cli_does_not_flag_ordinary_keys() {
    let dir = fresh_temp_dir("quoted-case-key-report-clean");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(case sym (apple :fruit) ((a b) :multi) (otherwise :x))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("quoted-case-key")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no quoted key in a `case` form"
        // from "no `case` form at all".
        .stdout(predicate::str::contains("\"case_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_flags_an_ecase_quoted_key() {
    let dir = fresh_temp_dir("quoted-case-key-report-ecase");
    let file = dir.join("a.lisp");
    fs::write(&file, "(ecase sym ('a 1))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("quoted-case-key")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"head\": \"ecase\""));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("quoted-case-key-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(case sym ('a 1))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "quoted-case-key", "--output", "json"])
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
fn cli_quoted_case_key_emits_sarif() {
    let dir = fresh_temp_dir("quoted-case-key-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case sym ('apple :fruit))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "quoted-case-key", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/quoted-case-key/quoted-case-key\"",
        ))
        .stdout(predicate::str::contains(
            "case key 'apple is quoted; case keys are not evaluated",
        ));
}

#[test]
fn cli_quoted_case_key_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("quoted-case-key-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(case x ((quote a) 1))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("quoted-case-key")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "quoted-case-key-report policy failed",
        ));
}

#[test]
fn cli_quoted_case_key_expands_directory_inputs() {
    let dir = fresh_temp_dir("quoted-case-key-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun f (x) (case x ('a 1)))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("quoted-case-key")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
