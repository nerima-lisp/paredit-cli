use super::*;

#[test]
fn cli_flags_gethash_nil_default() {
    let dir = fresh_temp_dir("gethash-default-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(gethash k table nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("gethash-default")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"gethash_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        // The fix's span, which the old report published and this one keeps.
        .stdout(predicate::str::contains("\"removal_span\""));
}

#[test]
fn cli_does_not_flag_non_nil_default() {
    let dir = fresh_temp_dir("gethash-default-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(gethash k table 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("gethash-default")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no explicit nil default in a
        // `gethash` form" from "no `gethash` form at all".
        .stdout(predicate::str::contains("\"gethash_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_gethash_default_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("gethash-default-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(gethash k table nil)\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "gethash-default", "--output", "json"])
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
fn cli_gethash_default_emits_sarif() {
    let dir = fresh_temp_dir("gethash-default-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(gethash k table nil)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "gethash-default", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/gethash-default/gethash-default\"",
        ))
        .stdout(predicate::str::contains("the gethash default is nil"));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("gethash-default-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(gethash k h nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("gethash-default")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "gethash-default-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_default() {
    let dir = fresh_temp_dir("gethash-default-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(gethash k table nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("gethash-default")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(gethash k table)\n");
}
