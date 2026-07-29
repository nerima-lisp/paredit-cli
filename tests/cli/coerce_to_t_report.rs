use super::*;

#[test]
fn cli_flags_coerce_to_t() {
    let dir = fresh_temp_dir("coerce-to-t-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(coerce value t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("coerce-to-t")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"coerce_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"object_span\""));
}

#[test]
fn cli_does_not_flag_real_coercion() {
    let dir = fresh_temp_dir("coerce-to-t-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(coerce x 'list)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("coerce-to-t")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no coercion to t in one `coerce`
        // form" from "no `coerce` form at all".
        .stdout(predicate::str::contains("\"coerce_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("coerce-to-t-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn id [x] (coerce x t))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "coerce-to-t", "--output", "json"])
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
fn cli_coerce_to_t_emits_sarif() {
    let dir = fresh_temp_dir("coerce-to-t-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(coerce x t)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "coerce-to-t", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/coerce-to-t/coerce-to-t\"",
        ))
        .stdout(predicate::str::contains(
            "coerce to type t returns the object unchanged; (coerce x t) is x",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("coerce-to-t-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(coerce x t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("coerce-to-t")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("coerce-to-t-report policy failed"));
}

#[test]
fn cli_lint_fix_unwraps() {
    let dir = fresh_temp_dir("coerce-to-t-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(coerce (compute x) t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("coerce-to-t")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(compute x)\n");
}
