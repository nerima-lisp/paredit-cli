use super::*;

#[test]
fn cli_flags_nested() {
    let dir = fresh_temp_dir("nested-string-case-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(string-upcase (string-downcase s))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-string-case")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        // Both the outer and the inner case op are forms scanned.
        .stdout(predicate::str::contains("\"string_case_form_count\": 2"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"outer_span\""))
        .stdout(predicate::str::contains("\"string_span\""));
}

#[test]
fn cli_does_not_flag_single() {
    let dir = fresh_temp_dir("nested-string-case-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(string-upcase s)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-string-case")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "one case op that is not nested"
        // from "no case op at all".
        .stdout(predicate::str::contains("\"string_case_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("nested-string-case-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [s] (string.upper s))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "nested-string-case", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_nested_string_case_emits_sarif() {
    let dir = fresh_temp_dir("nested-string-case-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(string-upcase (string-downcase s))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "nested-string-case", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/nested-string-case/nested-string-case\"",
        ))
        .stdout(predicate::str::contains(
            "the outer string case op dominates; the inner one is dead work",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("nested-string-case-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(string-upcase (string-downcase s))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-string-case")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "nested-string-case-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_collapses() {
    let dir = fresh_temp_dir("nested-string-case-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(string-downcase (string-capitalize name))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("nested-string-case")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(string-downcase name)\n");
}
