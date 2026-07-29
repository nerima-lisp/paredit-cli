use super::*;

#[test]
fn cli_flags_values_list_of_list() {
    let dir = fresh_temp_dir("values-list-of-list-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(values-list (list a b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("values-list-of-list")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"values_list_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"elements_span\""));
}

/// An empty `(list)` published `"elements_span": null`, and still does: a
/// consumer testing for the key would otherwise read the case as absent.
#[test]
fn cli_values_list_of_empty_list_keeps_a_null_element_span() {
    let dir = fresh_temp_dir("values-list-of-list-report-empty");
    let file = dir.join("a.lisp");
    fs::write(&file, "(values-list (list))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "values-list-of-list", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"elements_span\": null"));
}

#[test]
fn cli_does_not_flag_quoted_list() {
    let dir = fresh_temp_dir("values-list-of-list-report-quoted");
    let file = dir.join("a.lisp");
    fs::write(&file, "(values-list '(a b))\n(values-list xs)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("values-list-of-list")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "neither values-list spreads a
        // fresh list" from "there is no values-list at all".
        .stdout(predicate::str::contains("\"values_list_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_values_list_of_list_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("values-list-of-list-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [a b] (values-list (list a b)))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "values-list-of-list", "--output", "json"])
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
fn cli_values_list_of_list_emits_sarif() {
    let dir = fresh_temp_dir("values-list-of-list-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(values-list (list a b))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "values-list-of-list", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/values-list-of-list/throwaway-list\"",
        ))
        .stdout(predicate::str::contains(
            "values-list of a fresh list is just values",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("values-list-of-list-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(values-list (list a b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("values-list-of-list")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "values-list-of-list-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_values() {
    let dir = fresh_temp_dir("values-list-of-list-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(values-list (list (car x) (cdr x)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("values-list-of-list")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(values (car x) (cdr x))\n");
}
