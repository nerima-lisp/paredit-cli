use super::*;

#[test]
fn cli_flags_constant_true_test() {
    let dir = fresh_temp_dir("constant-if-test-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun pick () (if t 1 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("constant-if-test")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"if_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"test\": \"t\""));
}

#[test]
fn cli_does_not_flag_variable_or_truthy_literal_test() {
    let dir = fresh_temp_dir("constant-if-test-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if ready a b)\n(if 5 a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("constant-if-test")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no constant test in two `if`
        // forms" from "no `if` form at all".
        .stdout(predicate::str::contains("\"if_form_count\": 2"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// The test literal leads the text row as the finding's `kind`, so it must not
/// also appear as a `test=` column — that printed `t` twice on one line. The
/// JSON keeps the field, having no leading-kind column for it to duplicate.
#[test]
fn cli_text_rows_do_not_repeat_the_leading_kind() {
    let dir = fresh_temp_dir("constant-if-test-report-text");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun pick () (if t 1 2))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("constant-if-test")
        .arg("--output")
        .arg("text")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("test=").not())
        .stdout(predicate::str::contains("finding_count\t1"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("constant-if-test-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn pick [] (if true 1 2))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "constant-if-test", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_constant_if_test_emits_sarif() {
    let dir = fresh_temp_dir("constant-if-test-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun pick () (if nil 1 2))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "constant-if-test", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/constant-if-test/nil\"",
        ))
        .stdout(predicate::str::contains(
            "if test is the constant nil; one branch is dead",
        ));
}

#[test]
fn cli_constant_if_test_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("constant-if-test-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if nil dead alive)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("constant-if-test")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "constant-if-test-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_the_dead_branch() {
    let dir = fresh_temp_dir("constant-if-test-report-fix");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(if t (do-a x) (do-b y))\n(if nil (do-a x) (do-b y))\n(if nil (side-effect))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("constant-if-test")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(do-a x)\n(do-b y)\nnil\n");
}

#[test]
fn cli_lint_marks_constant_if_test_as_dead_code() {
    let dir = fresh_temp_dir("constant-if-test-report-lint");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if t a b)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("constant-if-test")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"category\": \"dead-code\""))
        .stdout(predicate::str::contains("\"fixable\": true"));
}
