use super::*;

#[test]
fn cli_flags_eq_against_nil() {
    let dir = fresh_temp_dir("nil-comparison-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun done? (x) (eq x nil))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nil-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"comparison_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"kind\": \"eq\""))
        .stdout(predicate::str::contains("\"operator\": \"eq\""));
}

#[test]
fn cli_flags_equal_with_nil_first() {
    let dir = fresh_temp_dir("nil-comparison-report-first");
    let file = dir.join("a.lisp");
    fs::write(&file, "(equal nil (lookup k))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nil-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"equal\""));
}

#[test]
fn cli_does_not_flag_numeric_equal_both_nil_or_quoted_nil() {
    let dir = fresh_temp_dir("nil-comparison-report-clean");
    let file = dir.join("a.lisp");
    // (= x nil) is numeric, (eq nil nil) is degenerate, 'nil is quoted, (eq a b) has no nil.
    fs::write(&file, "(= x nil)\n(eq nil nil)\n(eq x 'nil)\n(eq a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nil-comparison")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "three object comparisons, none
        // against nil" from "no comparison at all"; `(= x nil)` is numeric and
        // is not one of the forms this rule scans.
        .stdout(predicate::str::contains("\"comparison_form_count\": 3"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_nil_comparison_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("nil-comparison-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn done? [x] (eq x nil))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "nil-comparison", "--output", "json"])
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
fn cli_nil_comparison_emits_sarif() {
    let dir = fresh_temp_dir("nil-comparison-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eql result nil)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "nil-comparison", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/nil-comparison/eql\"",
        ))
        .stdout(predicate::str::contains(
            "eql against nil is a null test; use (null X)",
        ));
}

#[test]
fn cli_nil_comparison_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("nil-comparison-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(eql result nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nil-comparison")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "nil-comparison-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_nil_comparison_as_null() {
    let dir = fresh_temp_dir("nil-comparison-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(when (eq (rest xs) nil) (finish))\n").expect("write a.lisp");

    // The aggregator's --fix engine should rewrite (eq (rest xs) nil) to (null (rest xs)).
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("nil-comparison")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(when (null (rest xs)) (finish))\n");
}
