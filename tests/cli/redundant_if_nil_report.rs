use super::*;

#[test]
fn cli_flags_a_nil_else_branch() {
    let dir = fresh_temp_dir("redundant-if-nil-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (c x) (if c x nil))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-if-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"if_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"));
}

#[test]
fn cli_does_not_flag_two_arg_or_non_nil_else() {
    let dir = fresh_temp_dir("redundant-if-nil-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if c x)\n(if c x y)\n(if c nil nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-if-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no redundant nil else among three
        // `if` forms" from "no `if` form at all".
        .stdout(predicate::str::contains("\"if_form_count\": 3"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("redundant-if-nil-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [c x] (if c x nil))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "redundant-if-nil", "--output", "json"])
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
fn cli_redundant_if_nil_emits_sarif() {
    let dir = fresh_temp_dir("redundant-if-nil-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun f (c x) (if c x nil))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "redundant-if-nil", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/redundant-if-nil/redundant-if-nil\"",
        ))
        .stdout(predicate::str::contains(
            "if else branch is a redundant nil; (if c x nil) is (if c x)",
        ));
}

#[test]
fn cli_redundant_if_nil_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-if-nil-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if ready (go) nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-if-nil")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-if-nil-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_the_redundant_nil_else() {
    let dir = fresh_temp_dir("redundant-if-nil-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if c x nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--fix")
        .arg("--rule")
        .arg("redundant-if-nil")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"fixes_applied\": 1"));

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(if c x)\n");
}

#[test]
fn cli_redundant_if_nil_expands_directory_inputs() {
    let dir = fresh_temp_dir("redundant-if-nil-report-dir");
    let file = dir.join("nested.lisp");
    fs::write(&file, "(defun g (c) (if c (compute) nil))\n").expect("write nested.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-if-nil")
        .arg("--dialect")
        .arg("common-lisp")
        .arg("--output")
        .arg("json")
        .arg(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"));
}
