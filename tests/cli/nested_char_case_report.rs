use super::*;

#[test]
fn cli_flags_nested_char_case() {
    let dir = fresh_temp_dir("nested-char-case-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char-upcase (char-downcase c))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-char-case")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        // Both the outer upcase and the inner downcase are case-op forms scanned.
        .stdout(predicate::str::contains("\"char_case_form_count\": 2"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"outer_span\""))
        .stdout(predicate::str::contains("\"char_span\""));
}

#[test]
fn cli_does_not_flag_single_case() {
    let dir = fresh_temp_dir("nested-char-case-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char-upcase c)\n(char-downcase d)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-char-case")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no nesting in two case ops" from
        // "no case op at all".
        .stdout(predicate::str::contains("\"char_case_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("nested-char-case-report-unmodelled");
    let file = dir.join("a.clj");
    fs::write(&file, "(char-upcase (char-downcase c))\n").expect("write a.clj");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-char-case")
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
fn cli_nested_char_case_emits_sarif() {
    let dir = fresh_temp_dir("nested-char-case-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char-upcase (char-downcase c))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-char-case")
        .arg("--output")
        .arg("sarif")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/nested-char-case/nested-char-case\"",
        ))
        .stdout(predicate::str::contains(
            "the outer char case op dominates; the inner one is dead work",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("nested-char-case-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char-upcase (char-downcase c))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-char-case")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "nested-char-case-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_collapses_to_outer() {
    let dir = fresh_temp_dir("nested-char-case-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char-upcase (char-downcase c))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("nested-char-case")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(char-upcase c)\n");
}
