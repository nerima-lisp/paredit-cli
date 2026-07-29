use super::*;

#[test]
fn cli_flags_single_t_clause() {
    let dir = fresh_temp_dir("cond-t-clause-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond (t a b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("cond-t-clause")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"cond_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"body_span\""));
}

#[test]
fn cli_does_not_flag_multi_clause() {
    let dir = fresh_temp_dir("cond-t-clause-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond (p a) (t b))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("cond-t-clause")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no unconditional cond in one
        // `cond` form" from "no `cond` form at all".
        .stdout(predicate::str::contains("\"cond_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_cond_t_clause_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("cond-t-clause-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn pick [] (cond (t a b)))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "cond-t-clause", "--output", "json"])
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
fn cli_cond_t_clause_emits_sarif() {
    let dir = fresh_temp_dir("cond-t-clause-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond (t a b))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "cond-t-clause", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/cond-t-clause/cond-t-clause\"",
        ))
        .stdout(predicate::str::contains(
            "single t-clause cond is just progn",
        ));
}

#[test]
fn cli_cond_t_clause_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("cond-t-clause-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond (t x))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("cond-t-clause")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "cond-t-clause-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_progn() {
    let dir = fresh_temp_dir("cond-t-clause-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(cond (t (a) (b)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("cond-t-clause")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(progn (a) (b))\n");
}
