use super::*;

#[test]
fn cli_flags_explicit_nil() {
    let dir = fresh_temp_dir("getf-default-nil-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(getf plist :key nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("getf-default-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"call_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        // The fix's span, which the old report published and this one keeps.
        .stdout(predicate::str::contains("\"removal_span\""));
}

#[test]
fn cli_does_not_flag_non_nil() {
    let dir = fresh_temp_dir("getf-default-nil-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(getf plist :key 0)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("getf-default-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no explicit nil default in a
        // `getf` call" from "no `getf` call at all".
        .stdout(predicate::str::contains("\"call_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_getf_default_nil_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("getf-default-nil-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(getf plist :key nil)\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "getf-default-nil", "--output", "json"])
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
fn cli_getf_default_nil_emits_sarif() {
    let dir = fresh_temp_dir("getf-default-nil-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(getf plist :key nil)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "getf-default-nil", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/getf-default-nil/getf-default-nil\"",
        ))
        .stdout(predicate::str::contains(
            "explicit nil default restates getf's default",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("getf-default-nil-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(getf plist :key nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("getf-default-nil")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "getf-default-nil-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_default_nil() {
    let dir = fresh_temp_dir("getf-default-nil-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(getf plist :key nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("getf-default-nil")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(getf plist :key)\n");
}
