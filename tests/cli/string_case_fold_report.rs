use super::*;

#[test]
fn cli_flags_downcase_both() {
    let dir = fresh_temp_dir("string-case-fold-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(string= (string-downcase a) (string-downcase b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("string-case-fold")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"compare_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        // Both operand spans survived the move; the fix is reconstructible
        // from the report alone.
        .stdout(predicate::str::contains("\"left_span\""))
        .stdout(predicate::str::contains("\"right_span\""));
}

#[test]
fn cli_does_not_flag_mixed() {
    let dir = fresh_temp_dir("string-case-fold-report-clean");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(string= (string-downcase a) (string-upcase b))\n(string= a b)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("string-case-fold")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "neither comparison folds both
        // sides the same way" from "there is no string= at all".
        .stdout(predicate::str::contains("\"compare_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_string_case_fold_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("string-case-fold-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(
        &file,
        "(fn f [a b] (string= (string-downcase a) (string-downcase b)))\n",
    )
    .expect("write a.fnl");

    paredit()
        .args(["inspect", "string-case-fold", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_string_case_fold_emits_sarif() {
    let dir = fresh_temp_dir("string-case-fold-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(string= (string-downcase a) (string-downcase b))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "string-case-fold", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/string-case-fold/string-case-fold\"",
        ))
        .stdout(predicate::str::contains(
            "case-folding both sides of string= is case-insensitive; use string-equal",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("string-case-fold-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(string= (string-downcase a) (string-downcase b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("string-case-fold")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "string-case-fold-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_string_equal() {
    let dir = fresh_temp_dir("string-case-fold-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(string= (string-downcase a) (string-downcase b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("string-case-fold")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(string-equal a b)\n");
}
