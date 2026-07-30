use super::*;

#[test]
fn cli_flags_one_armed_if() {
    let dir = fresh_temp_dir("one-armed-if-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun run (ready) (if ready (go)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("one-armed-if")
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
fn cli_does_not_flag_two_armed_or_short_if() {
    let dir = fresh_temp_dir("one-armed-if-report-clean");
    let file = dir.join("a.lisp");
    // Two-armed if, argument-short if, and a reader-conditional operand.
    fs::write(&file, "(if test a b)\n(if test)\n(if #+sbcl a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("one-armed-if")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no one-armed if among three `if`
        // forms" from "no `if` form at all".
        .stdout(predicate::str::contains("\"if_form_count\": 3"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("one-armed-if-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn run [ready] (if ready (go)))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "one-armed-if", "--output", "json"])
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
fn cli_one_armed_if_emits_sarif() {
    let dir = fresh_temp_dir("one-armed-if-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun run (ready) (if ready (go)))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "one-armed-if", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/one-armed-if/one-armed-if\"",
        ))
        .stdout(predicate::str::contains(
            "if has no else branch; (if test then) is (when test then)",
        ));
}

#[test]
fn cli_one_armed_if_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("one-armed-if-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if ready (start))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("one-armed-if")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "one-armed-if-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_one_armed_if_as_when() {
    let dir = fresh_temp_dir("one-armed-if-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if ready (start))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("one-armed-if")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(when ready (start))\n");
}

#[test]
fn cli_lint_fix_composes_with_redundant_body_progn() {
    let dir = fresh_temp_dir("one-armed-if-report-fixpoint");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if ready (progn (a) (b)))\n").expect("write a.lisp");

    // one-armed-if turns it into (when ready (progn (a) (b))); the follow-on
    // redundant-body-progn fix then splices the progn during the same fixpoint.
    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("one-armed-if")
        .arg("--rule")
        .arg("redundant-body-progn")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(when ready (a) (b))\n");
}
