use super::*;

#[test]
fn cli_flags_list_star_nil() {
    let dir = fresh_temp_dir("list-star-nil-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list* a b nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("list-star-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"call_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        // Both spans the two-part rewrite needs, which the old report
        // published.
        .stdout(predicate::str::contains("\"head_span\""))
        .stdout(predicate::str::contains("\"removal_span\""));
}

#[test]
fn cli_does_not_flag_non_nil_tail() {
    let dir = fresh_temp_dir("list-star-nil-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list* a b)\n(list* nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("list-star-nil")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no nil tail in two `list*` calls"
        // from "no `list*` call at all".
        .stdout(predicate::str::contains("\"call_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("list-star-nil-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(list* a b nil)\n").expect("write a.clj");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("list-star-nil")
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
fn cli_list_star_nil_emits_sarif() {
    let dir = fresh_temp_dir("list-star-nil-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list* a b nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("list-star-nil")
        .arg("--output")
        .arg("sarif")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/list-star-nil/list-star-nil\"",
        ))
        .stdout(predicate::str::contains(
            "list* with a nil tail is a spelled-out list; (list* a b nil) is (list a b)",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("list-star-nil-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list* a b nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("list-star-nil")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "list-star-nil-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_list() {
    let dir = fresh_temp_dir("list-star-nil-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(list* a b nil)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("list-star-nil")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(list a b)\n");
}
