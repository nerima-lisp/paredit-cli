use super::*;

#[test]
fn cli_flags_mvl_of_values() {
    let dir = fresh_temp_dir("multiple-value-list-of-values-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(multiple-value-list (values a b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("multiple-value-list-of-values")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"mvl_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"elements_span\""));
}

#[test]
fn cli_does_not_flag_variable_arg() {
    let dir = fresh_temp_dir("multiple-value-list-of-values-report-variable");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(multiple-value-list (compute))\n(multiple-value-list (floor x y))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("multiple-value-list-of-values")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "neither of these two
        // `multiple-value-list` forms wraps a literal `values`" from "there is
        // no `multiple-value-list` at all".
        .stdout(predicate::str::contains("\"mvl_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("multiple-value-list-of-values-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [a b] (multiple-value-list (values a b)))\n").expect("write a.fnl");

    paredit()
        .args([
            "inspect",
            "multiple-value-list-of-values",
            "--output",
            "json",
        ])
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
fn cli_multiple_value_list_of_values_emits_sarif() {
    let dir = fresh_temp_dir("multiple-value-list-of-values-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(multiple-value-list (values a b))\n").expect("write a.lisp");

    paredit()
        .args([
            "inspect",
            "multiple-value-list-of-values",
            "--output",
            "sarif",
        ])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/multiple-value-list-of-values/multiple-value-list-of-values\"",
        ))
        .stdout(predicate::str::contains(
            "multiple-value-list of a values form is just list",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("multiple-value-list-of-values-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(multiple-value-list (values a b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("multiple-value-list-of-values")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "multiple-value-list-of-values-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_list() {
    let dir = fresh_temp_dir("multiple-value-list-of-values-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(multiple-value-list (values (car x) (cdr x)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("multiple-value-list-of-values")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(list (car x) (cdr x))\n");
}
