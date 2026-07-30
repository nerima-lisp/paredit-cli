use super::*;

#[test]
fn cli_flags_downcase_both() {
    let dir = fresh_temp_dir("char-case-fold-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char= (char-downcase a) (char-downcase b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("char-case-fold")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"compare_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"left_span\""))
        .stdout(predicate::str::contains("\"right_span\""));
}

#[test]
fn cli_does_not_flag_mixed() {
    let dir = fresh_temp_dir("char-case-fold-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char= (char-downcase a) (char-upcase b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("char-case-fold")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no case-folded pair in one
        // `char=`" from "no `char=` at all".
        .stdout(predicate::str::contains("\"compare_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("char-case-fold-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn same [a b] (= a b))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "char-case-fold", "--output", "json"])
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
fn cli_char_case_fold_emits_sarif() {
    let dir = fresh_temp_dir("char-case-fold-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char= (char-downcase a) (char-downcase b))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "char-case-fold", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/char-case-fold/char-case-fold\"",
        ))
        .stdout(predicate::str::contains(
            "case-folding both sides of char= is case-insensitive; use char-equal",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("char-case-fold-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char= (char-downcase a) (char-downcase b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("char-case-fold")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "char-case-fold-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_char_equal() {
    let dir = fresh_temp_dir("char-case-fold-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(char= (char-downcase a) (char-downcase b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("char-case-fold")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(char-equal a b)\n");
}
