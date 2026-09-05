use super::*;

#[test]
fn cli_flags_if_nil_else() {
    let dir = fresh_temp_dir("if-to-unless-report");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if ready nil (do-work))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-to-unless")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 1"))
        .stdout(predicate::str::contains("\"if_form_count\": 1"))
        .stdout(predicate::str::contains("\"line\": 1"))
        // The two operand spans this report has always published.
        .stdout(predicate::str::contains("\"test_span\""))
        .stdout(predicate::str::contains("\"else_span\""));
}

#[test]
fn cli_does_not_flag_else_t() {
    let dir = fresh_temp_dir("if-to-unless-report-clean");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if c nil t)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-to-unless")
        .arg("--output")
        .arg("json")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"finding_count\": 0"))
        // The denominator is what separates "no rewritable `if` among one" from
        // "no `if` form at all".
        .stdout(predicate::str::contains("\"if_form_count\": 1"))
        .stdout(predicate::str::contains("\"dialect_modelled\": true"));
}

#[test]
fn cli_labels_a_dialect_the_rule_does_not_model() {
    let dir = fresh_temp_dir("if-to-unless-report-unmodelled");
    let file = dir.join("a.fnl");
    fs::write(&file, "(fn go [] (if ready nil (do-work)))\n").expect("write a.fnl");

    paredit()
        .args(["inspect", "if-to-unless", "--output", "json"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dialect_modelled\": false"))
        .stdout(predicate::str::contains("\"finding_count\": 0"));
}

#[test]
fn cli_if_to_unless_emits_sarif() {
    let dir = fresh_temp_dir("if-to-unless-report-sarif");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if ready nil (do-work))\n").expect("write a.lisp");

    paredit()
        .args(["inspect", "if-to-unless", "--output", "sarif"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"ruleId\": \"inspect/if-to-unless/if-to-unless\"",
        ))
        .stdout(predicate::str::contains(
            "an if with a nil then-branch is an unless",
        ));
}

#[test]
fn cli_if_to_unless_fail_on_violation_trips_gate() {
    let dir = fresh_temp_dir("if-to-unless-report-gate");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if c nil e)\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("if-to-unless")
        .arg("--fail-on-violation")
        .arg(&file)
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "if-to-unless-report policy failed",
        ));
}

#[test]
fn cli_lint_fix_rewrites_to_unless() {
    let dir = fresh_temp_dir("if-to-unless-report-fix");
    let file = dir.join("a.lisp");
    fs::write(&file, "(if ready nil (do-work))\n").expect("write a.lisp");

    let mut cmd = paredit();
    cmd.arg("inspect")
        .arg("lint")
        .arg("--rule")
        .arg("if-to-unless")
        .arg("--fix")
        .arg(&file)
        .assert()
        .success();

    let fixed = fs::read_to_string(&file).expect("read fixed file");
    assert_eq!(fixed, "(unless ready (do-work))\n");
}
