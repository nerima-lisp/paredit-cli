use super::*;

#[test]
fn cli_flags_zero_minus() {
    let dir = fresh_temp_dir("verbose-negation-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(defun negate (x) (- 0 x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("verbose-negation")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"arithmetic_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"kind\": \"verbose-negation\""));
}

#[test]
fn cli_does_not_flag_trailing_zero_float_or_other_constant() {
    let dir = fresh_temp_dir("verbose-negation-report-clean");
    let file = dir.join("a.lisp");
    // Trailing zero (identity), float -1.0, and a different constant.
    fs::write(&file, "(- x 0)\n(* x -1.0)\n(- 5 x)\n(* x -2)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("verbose-negation")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no long-hand negation in four
        // scanned forms" from "no `-`/`*` form at all".
        .stdout(predicate::str::contains("\"arithmetic_form_count\": 4"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("verbose-negation-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn negate [x] (- 0 x))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "verbose-negation", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

/// The envelope's interchange formats, which this report reached by moving onto
/// it. This finding carries no fields of its own, so `message` is the whole of
/// what a SARIF consumer gets.
#[test]
fn cli_verbose_negation_emits_sarif() {
    let dir = fresh_temp_dir("verbose-negation-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(* balance -1)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "verbose-negation", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/verbose-negation/verbose-negation\"",
        ))
        .stdout(predicate::str::contains(
            "negation written the long way; use (- x)",
        ));
}

#[test]
fn cli_verbose_negation_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("verbose-negation-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(* balance -1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("verbose-negation")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "verbose-negation-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_all_negation_shapes() {
    let dir = fresh_temp_dir("verbose-negation-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(- 0 (compute x))\n(* delta -1)\n(* -1 delta)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("verbose-negation")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(- (compute x))\n(- delta)\n(- delta)\n");
}
