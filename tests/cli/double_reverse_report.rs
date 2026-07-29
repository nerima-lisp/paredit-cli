use super::*;

#[test]
fn cli_flags_double_reverse() {
    let dir = fresh_temp_dir("double-reverse-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(reverse (reverse xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("double-reverse")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"reverse_form_count\": 2"))
        .stdout(predicate::str::contains("\"line\": 1"))
        // The inner argument's span is what a consumer needs to build the
        // (copy-seq x) rewrite itself, and the old report published it.
        .stdout(predicate::str::contains("\"inner_span\""));
}

#[test]
fn cli_does_not_flag_single_reverse_or_nreverse() {
    let dir = fresh_temp_dir("double-reverse-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(reverse xs)\n(nreverse (nreverse xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("double-reverse")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no double reverse in one
        // `reverse` form" from "no `reverse` form at all".
        .stdout(predicate::str::contains("\"reverse_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("double-reverse-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(reverse (reverse xs))\n").expect("write a.clj");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("double-reverse")
        .arg("--output")
        .arg("json")
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
fn cli_double_reverse_emits_sarif() {
    let dir = fresh_temp_dir("double-reverse-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(reverse (reverse xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("double-reverse")
        .arg("--output")
        .arg("sarif")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/double-reverse/double-reverse\"",
        ))
        .stdout(predicate::str::contains(
            "(reverse (reverse x)) is a wasteful copy; use (copy-seq x)",
        ));
}

#[test]
fn cli_double_reverse_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("double-reverse-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(reverse (reverse xs))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("double-reverse")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "double-reverse-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_copy_seq() {
    let dir = fresh_temp_dir("double-reverse-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(reverse (reverse (mapcar #'f ys)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("double-reverse")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(copy-seq (mapcar #'f ys))\n");
}
