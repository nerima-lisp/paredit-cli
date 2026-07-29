use super::*;

#[test]
fn cli_flags_aesthetic() {
    let dir = fresh_temp_dir("format-to-string-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(format nil \"~A\" value)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("format-to-string")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"format_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"argument_span\""))
        .stdout(predicate::str::contains(
            "\"replacement\": \"princ-to-string\"",
        ));
}

#[test]
fn cli_does_not_flag_surrounding_text() {
    let dir = fresh_temp_dir("format-to-string-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(format nil \"value: ~A\" x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("format-to-string")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "one format call whose control
        // string carries more than the directive" from "no format call".
        .stdout(predicate::str::contains("\"format_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("format-to-string-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [x] (tostring x))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "format-to-string", "--output", "json"])
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
fn cli_format_to_string_emits_sarif() {
    let dir = fresh_temp_dir("format-to-string-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(format nil \"~S\" x)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "format-to-string", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/format-to-string/prin1-to-string\"",
        ))
        .stdout(predicate::str::contains(
            "format to a string is just prin1-to-string; use (prin1-to-string x)",
        ));
}

#[test]
fn cli_format_to_string_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("format-to-string-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(format nil \"~S\" x)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("format-to-string")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "format-to-string-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_princ_to_string() {
    let dir = fresh_temp_dir("format-to-string-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(format nil \"~A\" (thing x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("format-to-string")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(princ-to-string (thing x))\n");
}
