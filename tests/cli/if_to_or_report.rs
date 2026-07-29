use super::*;

#[test]
fn cli_flags_if_x_x_y() {
    let dir = fresh_temp_dir("if-to-or-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if cached cached (compute))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-to-or")
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
fn cli_does_not_flag_compound_or_constant_test() {
    let dir = fresh_temp_dir("if-to-or-report-clean");
    let file = dir.join("a.lisp");
    // A differing then, a compound (double-eval) test, and literal t/nil tests
    // are all left alone.
    fs::write(
        &file,
        "(if a b c)\n(if (pop s) (pop s) y)\n(if t t y)\n(if nil nil y)\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-to-or")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "all four `if` forms here are
        // left alone deliberately" from "no `if` form at all".
        .stdout(predicate::str::contains("\"if_form_count\": 4"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("if-to-or-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn f [x y] (if x x y))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "if-to-or", "--output", "json"])
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
fn cli_if_to_or_emits_sarif() {
    let dir = fresh_temp_dir("if-to-or-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if cached cached (compute))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "if-to-or", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/if-to-or/if-to-or\"",
        ))
        .stdout(predicate::str::contains(
            "if returns its test or the else; (if x x y) is (or x y)",
        ));
}

#[test]
fn cli_if_to_or_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("if-to-or-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if found found (search))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-to-or")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("if-to-or-report policy failed"));
}

#[test]
fn cli_lint_fix_rewrites_as_or() {
    let dir = fresh_temp_dir("if-to-or-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if cached cached (compute x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("if-to-or")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(or cached (compute x))\n");
}
