use super::*;

#[test]
fn cli_flags_roundtrip() {
    let dir = fresh_temp_dir("code-char-char-code-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(code-char (char-code c))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("code-char-char-code")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"code_char_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"char_span\""));
}

#[test]
fn cli_does_not_flag_reverse() {
    let dir = fresh_temp_dir("code-char-char-code-report-reverse");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char-code (code-char n))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("code-char-char-code")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "one `code-char` that is not a
        // round-trip" from "no `code-char` at all".
        .stdout(predicate::str::contains("\"code_char_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("code-char-char-code-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [c] c)\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "code-char-char-code", "--output", "json"])
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
fn cli_code_char_char_code_emits_sarif() {
    let dir = fresh_temp_dir("code-char-char-code-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(code-char (char-code c))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "code-char-char-code", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/code-char-char-code/code-char-char-code\"",
        ))
        .stdout(predicate::str::contains(
            "code-char of char-code is a round-trip; (code-char (char-code c)) is c",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("code-char-char-code-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(code-char (char-code c))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("code-char-char-code")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "code-char-char-code-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_unwraps() {
    let dir = fresh_temp_dir("code-char-char-code-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(code-char (char-code (elt s i)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("code-char-char-code")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(elt s i)\n");
}
