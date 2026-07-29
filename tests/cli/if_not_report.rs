use super::*;

#[test]
fn cli_flags_if_nil_t() {
    let dir = fresh_temp_dir("if-not-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if ready nil t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-not")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"if_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        // The test span is the rewrite's input, but the report has always
        // published it.
        .stdout(predicate::str::contains("\"test_span\""));
}

#[test]
fn cli_does_not_flag_boolean_coercion() {
    let dir = fresh_temp_dir("if-not-report-clean");
    let file = dir.join("a.lisp");
    // (if test t nil) is a coercion with no clearer single builtin.
    fs::write(&file, "(if x t nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-not")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "the one `if` here is a coercion"
        // from "no `if` form at all".
        .stdout(predicate::str::contains("\"if_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("if-not-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [x] (if x nil t))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "if-not", "--output", "json"])
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
fn cli_if_not_emits_sarif() {
    let dir = fresh_temp_dir("if-not-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if ready nil t)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "if-not", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/if-not/if-not\"",
        ))
        .stdout(predicate::str::contains(
            "if with then=nil and else=t is a negation",
        ));
}

#[test]
fn cli_if_not_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("if-not-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if x nil t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-not")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("if-not-report policy failed"));
}

#[test]
fn cli_lint_fix_rewrites_to_not() {
    let dir = fresh_temp_dir("if-not-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if (ready-p x) nil t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("if-not")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(not (ready-p x))\n");
}
