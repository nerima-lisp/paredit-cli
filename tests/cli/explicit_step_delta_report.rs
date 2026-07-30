use super::*;

#[test]
fn cli_flags_incf_explicit_one() {
    let dir = fresh_temp_dir("explicit-step-delta-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(incf counter 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("explicit-step-delta")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"step_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"operator\": \"incf\""));
}

#[test]
fn cli_does_not_flag_non_unit_float_or_implicit_delta() {
    let dir = fresh_temp_dir("explicit-step-delta-report-clean");
    let file = dir.join("a.lisp");
    // A non-1 delta, a float 1.0 (type-coercing), and the implicit default are
    // all left alone.
    fs::write(&file, "(incf x 2)\n(incf y 1.0)\n(decf z)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("explicit-step-delta")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no redundant delta in three step
        // forms" from "no step form at all".
        .stdout(predicate::str::contains("\"step_form_count\": 3"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("explicit-step-delta-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn bump [n] (+ n 1))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "explicit-step-delta", "--output", "json"])
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
fn cli_explicit_step_delta_emits_sarif() {
    let dir = fresh_temp_dir("explicit-step-delta-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(decf remaining 1)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "explicit-step-delta", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        // The macro is the finding's kind, so `incf` and `decf` are separable
        // by rule id without parsing the JSON body.
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/explicit-step-delta/decf\"",
        ))
        .stdout(predicate::str::contains(
            "decf delta of 1 is the default; (decf x 1) is (decf x)",
        ));
}

#[test]
fn cli_explicit_step_delta_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("explicit-step-delta-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(decf remaining 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("explicit-step-delta")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "explicit-step-delta-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_the_default_delta() {
    let dir = fresh_temp_dir("explicit-step-delta-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(incf (aref v i) 1)\n(decf n 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("explicit-step-delta")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(incf (aref v i))\n(decf n)\n");
}
