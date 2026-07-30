use super::*;

#[test]
fn cli_flags_explicit_one() {
    let dir = fresh_temp_dir("butlast-default-count-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(butlast xs 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("butlast-default-count")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"call_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        .stdout(predicate::str::contains("\"removal_span\""));
}

#[test]
fn cli_does_not_flag_bare() {
    let dir = fresh_temp_dir("butlast-default-count-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(butlast xs)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("butlast-default-count")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no redundant count in one
        // `butlast` call" from "no `butlast` call at all".
        .stdout(predicate::str::contains("\"call_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

/// An empty finding list is ambiguous, so a dialect this rule does not model
/// must be labelled rather than silently reported as clean.
#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("butlast-default-count-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn head [xs] (butlast xs 1))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "butlast-default-count", "--output", "json"])
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
fn cli_butlast_default_count_emits_sarif() {
    let dir = fresh_temp_dir("butlast-default-count-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(nbutlast xs 1)\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "butlast-default-count", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/butlast-default-count/butlast-default-count\"",
        ))
        .stdout(predicate::str::contains(
            "explicit count of 1 restates butlast's default",
        ));
}

#[test]
fn cli_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("butlast-default-count-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(butlast xs 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("butlast-default-count")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "butlast-default-count-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_drops_default_count() {
    let dir = fresh_temp_dir("butlast-default-count-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(butlast xs 1)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("butlast-default-count")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(butlast xs)\n");
}
