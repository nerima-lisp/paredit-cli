use super::*;

#[test]
fn cli_flags_floor_by_one() {
    let dir = fresh_temp_dir("redundant-divisor-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(floor total 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-divisor")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"quotient_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"kind\": \"floor\""))
        .stdout(predicate::str::contains("\"operator\": \"floor\""))
        // A fix input, but the old report published it, so it stays published.
        .stdout(predicate::str::contains("\"number_span\""));
}

#[test]
fn cli_does_not_flag_non_one_divisor() {
    let dir = fresh_temp_dir("redundant-divisor-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(floor x 2)\n(mod x 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-divisor")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "one quotient form, correctly
        // divided" from "no quotient form at all"; `mod` is not one of the
        // operators this rule scans.
        .stdout(predicate::str::contains("\"quotient_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_redundant_divisor_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("redundant-divisor-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn half [x] (floor x 1))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "redundant-divisor", "--output", "json"])
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
fn cli_redundant_divisor_emits_sarif() {
    let dir = fresh_temp_dir("redundant-divisor-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(round x 1)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "redundant-divisor", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/redundant-divisor/round\"",
        ))
        .stdout(predicate::str::contains(
            "the divisor defaults to 1; (round x 1) is (round x)",
        ));
}

#[test]
fn cli_redundant_divisor_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("redundant-divisor-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(round x 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("redundant-divisor")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "redundant-divisor-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_divisor() {
    let dir = fresh_temp_dir("redundant-divisor-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(truncate (+ a b) 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("redundant-divisor")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(truncate (+ a b))\n");
}
