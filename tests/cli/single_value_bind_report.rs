use super::*;

#[test]
fn cli_flags_single_variable_bind() {
    let dir = fresh_temp_dir("single-value-bind-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(multiple-value-bind (q) (truncate a b) (use q))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-value-bind")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"bind_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"));
}

#[test]
fn cli_does_not_flag_multi_variable_or_empty_bind() {
    let dir = fresh_temp_dir("single-value-bind-report-clean");
    let file = dir.join("a.lisp");
    // Two variables capture secondary values; an empty list is a progn.
    fs::write(
        &file,
        "(multiple-value-bind (q r) (truncate a b) q)\n(multiple-value-bind () (f) done)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-value-bind")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "neither of two binds is
        // single-value" from "no bind at all".
        .stdout(predicate::str::contains("\"bind_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("single-value-bind-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(multiple-value-bind (x) (f) x)\n").expect("write a.clj");

    paredit()
        .args(["inspect", "single-value-bind", "--output", "json"])
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
fn cli_single_value_bind_emits_sarif() {
    let dir = fresh_temp_dir("single-value-bind-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(multiple-value-bind (v) (compute) (list v))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "single-value-bind", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/single-value-bind/single-value-bind\"",
        ))
        .stdout(predicate::str::contains(
            "multiple-value-bind of one variable is just let",
        ));
}

#[test]
fn cli_single_value_bind_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("single-value-bind-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(multiple-value-bind (v) (compute) (list v))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("single-value-bind")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "single-value-bind-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_bind_as_let() {
    let dir = fresh_temp_dir("single-value-bind-report-fix");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(multiple-value-bind (q) (truncate a b) (use q) (log q))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("single-value-bind")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(let ((q (truncate a b))) (use q) (log q))\n");
}
