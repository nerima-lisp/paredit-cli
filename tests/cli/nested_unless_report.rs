use super::*;

#[test]
fn cli_flags_unless_in_unless() {
    let dir = fresh_temp_dir("nested-unless-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(unless a (unless b (do-it)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-unless")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        // Both the outer and the inner `unless` are scanned.
        .stdout(predicate::str::contains("\"unless_form_count\": 2"))
        .stdout(predicate::str::contains("\"line\": 1"));
}

#[test]
fn cli_does_not_flag_extra_body_or_non_unless() {
    let dir = fresh_temp_dir("nested-unless-report-clean");
    let file = dir.join("a.lisp");
    // Extra outer body form (d not guarded by b); non-unless inner body.
    fs::write(&file, "(unless a (unless b c) d)\n(unless a (when b c))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-unless")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no nested unless in three unless
        // forms" from "no unless form at all".
        .stdout(predicate::str::contains("\"unless_form_count\": 3"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("nested-unless-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn go [] (when a (when b (do-it))))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "nested-unless", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_nested_unless_emits_sarif() {
    let dir = fresh_temp_dir("nested-unless-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(unless a (unless b (do-it)))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "nested-unless", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/nested-unless/nested-unless\"",
        ))
        .stdout(predicate::str::contains(
            "unless whose only body is an unless merges by or",
        ));
}

#[test]
fn cli_nested_unless_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("nested-unless-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(unless done (unless paused (tick)))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("nested-unless")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "nested-unless-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_merges_tests_with_or() {
    let dir = fresh_temp_dir("nested-unless-report-fix");
    let file = dir.join("a.lisp");
    fs::write(
        &file,
        "(unless (done-p x) (unless (< n 0) (step n) (log n)))\n",
    )
    .expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("nested-unless")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(unless (or (done-p x) (< n 0)) (step n) (log n))\n");
}
